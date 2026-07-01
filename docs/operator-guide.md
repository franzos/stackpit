# Operator Guide

Everything beyond getting the binary and starting it: the full configuration reference, authentication and OIDC setup, connecting SDKs, notifications, source maps, syncing, and the CLI. For install and first boot, see the [README](../README.md).

## Contents

- [Configuration](#configuration)
- [PostgreSQL](#postgresql)
- [Authentication](#authentication)
- [Organizations & Roles](#organizations--roles)
- [Secret encryption](#secret-encryption)
- [Connecting SDKs](#connecting-sdks)
- [Notifications & Alerts](#notifications--alerts)
- [Source Maps](#source-maps)
- [Monitors](#monitors)
- [Web UI](#web-ui)
- [Syncing from Sentry](#syncing-from-sentry)
- [CLI tools](#cli-tools)
- [Edge cases](#edge-cases)

## Configuration

Config lives in `stackpit.toml` (override with `-c /path/to/config.toml`).

```toml
[server]
bind = "127.0.0.1:3000"          # admin UI/API listener
ingest_bind = "0.0.0.0:3001"     # SDK ingestion listener
external_url = ""                 # public URL of the admin/UI surface (optional; used for OIDC + cookie Secure heuristics)
external_ingest_url = ""          # public URL of the ingest surface; falls back to external_url, then http://{ingest_bind}.
                                  # Set this when ingest lives on a different host or port than admin.
admin_token = ""                  # shared bearer token for admin auth (min 16 chars)
force_secure_cookies = false      # set true behind a TLS-terminating proxy on a non-loopback bind
no_auth_loopback_acknowledged = false  # required to run with no auth at all; loopback bind only
max_body_size = 10485760          # max decompressed body in bytes (default 10MB)
# max_compressed_body_size = ...  # max compressed body in bytes (default: max_body_size / 5)

[storage]
path = "stackpit.db"              # SQLite database path
database_url = ""                 # full URL, e.g. "postgres://user:pass@host/stackpit" (overrides path)
retention_days = 90               # auto-delete events older than this (0 = keep forever)

[filter]
mode = "open"                     # "open" = auto-provision new projects on first ingest; "closed" = pre-register everything
rate_limit = 0                    # global max events per minute (0 = unlimited)
max_projects = 1000               # max auto-registered projects in open mode
max_native_orgs_per_user = 10     # max organizations a single user can create
excluded_environments = []        # environment names to reject globally
blocked_user_agents = []          # user-agent glob patterns to block globally

[notifications]
rate_limit_per_project = 0        # max notifications per project per 60s (0 = unlimited)
rate_limit_global = 0             # max total notifications per 60s (0 = unlimited)

[email]
provider = "lettermint"           # "lettermint", "postmark", or "sendgrid" — default for new integrations, and the only provider used when lock = true
token = ""                        # global provider API token; integrations inherit it when they leave Token blank (required when lock = true)
from_address = ""                 # default sender; integrations inherit it when they leave From blank (required when lock = true)
from_name = ""                    # optional default display name
lock = false                      # true = sender + provider come from this config; integrations only pick the recipient
```

All fields have sane defaults. An empty config file works fine — though for any non-loopback deployment you'll want `external_url` set and `force_secure_cookies = true` so session cookies get the `Secure` flag.

**Filter modes — what `open` vs `closed` controls is ingest admission, not event filtering:**

- `open` — the first event for an unknown `project_id` auto-creates the project and registers the key it was sent with. After that, only registered keys are accepted; further keys must be added explicitly via the admin UI. Convenient for solo deployments and CI bring-up.
- `closed` — every project and every key must be created up-front via the admin UI. Unknown `project_id` or unknown key → reject.

Event-level filtering (message globs, IP CIDRs, rate limits, release/environment/user-agent rules, fingerprint discards) is layered on top of either mode and managed per-project in the web UI under **Filters**. Filtering runs in two tiers so cheap checks happen before expensive ones. Before the body is even parsed, the pre-filter runs per-key rate limits, user-agent blocks, and IP CIDR matches. Once the event is parsed, the event-level filter runs in order: fingerprint discards, built-in inbound filters (browser-extension/localhost), message globs, environment excludes, release filters, and finally custom filter rules.

## PostgreSQL

SQLite is the default, but you can point stackpit at a PostgreSQL database instead:

```toml
[storage]
database_url = "postgres://user:pass@localhost/stackpit"
```

When `database_url` is set it takes precedence over `path`. Migrations run automatically on startup for both backends.

## Authentication

Two paths, both optional, and they can run side-by-side:

- **Admin token** — a single shared bearer for solo deployments. Always works as a break-glass fallback, even when OAuth is on.
- **OAuth / SSO** — for multi-user setups where you delegate identity to a real IdP.

### Admin token

Set `admin_token` in `[server]` (`stackpit init` does this for you):

```toml
[server]
admin_token = "..."   # 64-char hex string (32 bytes) from `openssl rand -hex 32`, min 16 chars
```

Requests need either `Authorization: Bearer <token>` or a `stackpit_token` cookie (set via `/web/login`). The cookie stores SHA-256 of the token, never the raw value.

### OAuth / SSO

stackpit speaks standard OAuth 2.0 + OIDC, so it works with any compliant authorization server — [Ory Hydra](https://www.ory.sh/hydra/), Keycloak, Authentik, Auth0, and the like. It runs the Authorization Code + PKCE flow, verifies the id_token against the IdP's JWKS, and then issues its own session cookie (HttpOnly, SameSite=Strict). The IdP tokens themselves are stored encrypted in the `oidc_grants` table — stackpit owns session state from the callback onwards.

The example below uses Hydra, but the steps are the same for any provider: register stackpit as a confidential client, then point `[auth.oauth]` at the issuer. With Hydra:

```bash
hydra create oauth2-client \
  --endpoint https://hydra-admin.internal:4445 \
  --name "stackpit" \
  --grant-type authorization_code,refresh_token \
  --response-type code \
  --scope "openid email profile offline_access orgs" \
  --token-endpoint-auth-method client_secret_post \
  --audience stackpit-web \
  --redirect-uri https://stackpit.example.com/web/auth/callback
```

The `--audience` allow-list entry matters: stackpit sends `audience=stackpit-web` on the authorization request so Hydra binds it into the access token's `aud`, and the web gate then checks for it. Hydra only honours audiences that appear on the client's allow-list — leave it off and the token comes back without the `aud`, and every web session is rejected with `InvalidAudience`. (Hydra uses a non-standard `audience=` parameter for this; RFC 8707 `resource=` isn't wired in Hydra yet.)

The `orgs` scope in the example is optional. It's what lets stackpit map organizations and roles from your IdP; leave it off and you get personal-orgs-only. See [Organizations & Roles](#organizations--roles) for the full picture. It only works if the IdP is actually registered to grant the scope, so keep it on the client's allow-list too.

Then wire it into `stackpit.toml`:

```toml
[server]
external_url = "https://stackpit.example.com"   # required for OAuth — callback URLs are built from this

[auth.oauth]
issuer_url    = "https://hydra.example.com"
client_id     = "<from hydra output>"
client_secret = "<from hydra output>"
redirect_uri  = "https://stackpit.example.com/web/auth/callback"
web_audience  = "stackpit.example.com"                  # required — must match the IdP audience for the web client
```

`web_audience` binds the BFF to the audience your IdP issues to the web client; it blocks confused-deputy attacks across resource servers and is enforced at startup.

**Access-token validation: JWT vs opaque.** On every request the web gate validates the grant's access token. It supports both token shapes:

- **JWT access tokens (recommended, the default for Hydra).** Validated locally against the IdP's JWKS — signature, `iss`, `aud`, `exp`. No network call on the hot path, no admin-API reachability needed. As long as discovery advertises a `jwks_uri` (it normally does), this just works.
- **Opaque access tokens.** Can't be validated locally — they require RFC 7662 introspection. For these to work, **the IdP's discovery document must advertise an `introspection_endpoint`, or you must set `introspection_url` under `[auth.oauth]` manually.** Note Hydra only exposes introspection on its *admin* API (not in public discovery) and that API is private, so opaque-token setups need stackpit to have a network path to it.

If neither validator is available — no JWKS and no introspection URL — the web gate can't be built and SSO is disabled (stackpit logs an error at startup). If only JWKS is available (no introspection), stackpit logs a warning that opaque tokens will be rejected; this is fine for the JWT default but a misconfiguration if your IdP issues opaque tokens.

User rows are provisioned just-in-time on first login, linked by the OIDC `(iss, sub)` pair. What a user can see and do is scoped by organization and role, covered in the next section; the `admin_token` is a separate break-glass code path (CLI, headless ops), doesn't flow through the users table at all, and acts as a superuser above every org. Email is stored only when the IdP reports `email_verified=true`; unverified emails are ignored to keep an attacker-controlled string out of identity decisions.

A "Sign in with SSO" button appears on `/web/login` whenever `[auth.oauth]` is configured. The admin_token path keeps working alongside as a break-glass.

The full set of OAuth knobs (`post_logout_redirect_uri`, `access_token_max_ttl_secs`, `introspection_cache_ttl_secs`, `session_max_ttl_secs`, etc.) is documented inline in the config that `stackpit init` writes — read that file for the authoritative reference.

## Organizations & Roles

**This is an OIDC feature.** stackpit has no local user registration, no password signup: a user only exists once someone authenticates through your IdP (the `users` table is keyed on the OIDC `(iss, sub)` pair). So organizations, roles, membership, and invites are only meaningful when [OAuth / SSO](#oauth--sso) is configured. On an admin-token-only deployment the `admin_token` acts as a superuser that sees every org, and the org layer stays dormant: there are no member users to invite or scope.

With OIDC on, every logged-in user belongs to at least one organization, and data is scoped per org: you see the projects, issues, events, and settings for your active org, and nothing from other orgs. This replaces the old "everyone sees everything" behaviour, so a multi-user stackpit is now genuinely multi-tenant. Ingestion is unaffected: SDKs authenticate with a project key, and the project's org is what ties the incoming data to a tenant.

**Personal orgs, native orgs, and invites.** On first login you get your own personal organization and you're its owner. Beyond that, any logged-in user can create additional organizations from the **Organizations** page in the sidebar, up to a configurable per-user limit (`max_native_orgs_per_user`, default 10); you become the owner of whatever you create. To collaborate, an owner generates an invite link (with a role and an expiry) and whoever opens it while logged in joins that org. Invites cover personal and stackpit-native orgs. Orgs that come from your IdP (below) get their membership from claims instead, so they don't use invite links.

**Roles.** Within an org you're either an `owner` or a `member`:

- `owner` runs the org: create and delete projects, manage members and invites, edit filters, integrations, alerts, source-map keys, and settings.
- `member` is read-only across the org's projects, issues, and events. Members browse but don't change status, settings, or keys.

The `admin_token` sits above all of this as a superuser. It sees every org (including the system org below) and can do anything, which is why it stays the break-glass path.

**Active org and switching.** You have one active org per session. The **Organizations** page (in the sidebar) lists every org you belong to and lets you switch which one is active; every read and write is then scoped to whatever's active. The active org lives in a signed cookie, but your role is looked up live on each request, so a demotion or an IdP change takes effect without you having to clear anything.

**Auto-registered projects (open mode).** In `open` mode a brand-new DSN auto-creates its project, but there's no user or org in that request, so the project lands in a built-in system org called "Unassigned". Normal users never see it; only the `admin_token` superuser does, through the Unassigned view, where each project can be reassigned into a real org. New DSNs flow into a holding area you triage, rather than leaking into someone's org.

### Mapping orgs and roles from your IdP

If your IdP reports organization membership on its claims, stackpit maps those orgs and roles in on login, so you don't manage membership in two places. This is built around [Forseti](https://git.gofranz.com/franz/forseti)'s `orgs` claim, and any IdP that emits the same shape works.

To turn it on, grant stackpit's client the `orgs` scope (alongside `openid email profile offline_access`) and make sure the IdP is registered to grant it. Without the scope the claim never arrives, and stackpit logs a warning and falls back to personal-orgs-only.

On each login stackpit reads the full `orgs` claim (the user's memberships, each with a role) and reconciles:

- The IdP's **default org** maps to your personal stackpit org. Nobody ever joins the default org itself.
- For a **non-default org**, an owner is prompted to create the matching stackpit org the first time they log in. Once it exists, other members of that same IdP org join it automatically on their next login, with the role the claim gives them. Members can't create the org; they wait for an owner.
- The IdP is authoritative: removals and demotions propagate on the next login. Each mapped org has a per-org "sync roles" toggle if you'd rather freeze roles locally after the initial join.

A few safety rules keep this from misfiring. An absent or empty `orgs` claim never removes anything, so a scope misconfiguration can't evict everyone. A truncated membership list disables removals for that login. The last owner of an org is never demoted or removed by sync. And because the claim is snapshotted at consent time, stackpit forces a full re-login past `session_max_ttl_secs` (default 8h) so a demotion can't linger indefinitely behind a long-lived refresh token.

## Secret encryption

When OAuth is enabled, stackpit **requires** a 32-byte hex master key — OIDC tokens are stored encrypted at rest in `oidc_grants` and startup will refuse without it. The same key encrypts integration credentials (Slack tokens, webhook URLs). Without it and without OAuth, integration secrets fall back to plaintext storage and stackpit warns on startup.

Supply the key one of two ways:

```bash
export STACKPIT_MASTER_KEY=$(openssl rand -hex 32)
```

```toml
[server]
master_key = "..."   # 64-char hex string (32 bytes) from `openssl rand -hex 32`
```

The env var wins when both are set — so you can keep a key out of the config file entirely (systemd `EnvironmentFile`, a secrets manager) and still override a placeholder in `stackpit.toml`. That separation matters more than it looks: this key decrypts the secrets sitting in your database, so storing it next to the DB — same directory, same backup — defeats the point. Keep them apart if you can. A malformed key (bad hex, wrong length) fails startup fast from either source, naming whichever one it came from.

## Connecting SDKs

Any Sentry SDK works. The ingestion server speaks the standard Sentry protocol — envelope and legacy store endpoints, all auth methods (header, query param, DSN).

I've tested with the official SDKs for **JavaScript**, **Python**, **Rust**, **Go**, **Ruby**, **Java**, **C#/.NET**, **PHP**, and others. If it sends Sentry envelopes, it works.

### DSN format

```
https://<key>@<ingest-host>:<port>/<project-id>
```

For example, with the default ingestion port:

```
https://mykey@errors.example.com:3001/1
```

The host:port in the DSN comes from `external_ingest_url`, falling back to `external_url`, then `http://{ingest_bind}`. Behind a single reverse proxy that fronts both listeners on one origin, leave `external_ingest_url` unset. For split deployments or local split-port dev where admin and ingest are reachable on different hosts/ports, set both.

## Notifications & Alerts

stackpit can notify you when things go wrong. Integrations (email, Slack, webhooks) are configured in the web UI under **Settings → Integrations**, and each project can enable or disable specific triggers. Integrations, alert rules, and digest schedules all belong to your active organization (see [Organizations & Roles](#organizations--roles)); an "org-wide" alert or digest covers the projects in that org, not the whole instance.

Email goes through [polymail](https://github.com/franzos/polymail-rs) and supports **Lettermint**, **Postmark**, and **SendGrid**. By default you pick the provider per integration when you add it, then drop in that provider's API token and a from address; `[email] provider`/`from_address`/`from_name` set the defaults, and an integration that leaves a field blank inherits them. Per-integration tokens are stored encrypted (see [Secret encryption](#secret-encryption)).

For a single shared mailer, set `[email] lock = true`: the provider, token, and sender all come from `[email]`, and integrations then only choose the recipient — no per-integration token or sender. A locked mailer with no `token` or `from_address` refuses to start.

**Immediate notifications** fire during event ingestion:

- **New issue** — a fingerprint appears for the first time
- **Regression** — a previously resolved issue reappears
- **Threshold exceeded** — a custom alert rule fires (e.g. 100 events in 5 minutes)

**Digest emails** summarize activity over a configurable interval — new issues, active issue counts, and total events per project. Digest schedules can be per-project or global.

Each project integration can filter notifications by trigger type, minimum severity level, and environment. Rate limiting (configurable in `[notifications]`) prevents notification storms.

Alert rules and digest schedules are managed via the web UI under **Alerts**, or through the JSON API (`/api/v1/alerts/rules`, `/api/v1/digests`).

## Source Maps

stackpit supports source map uploads so minified stack traces resolve to original source locations.

Generate a project API key in **Settings → Source Maps**, then configure `sentry-cli` or your bundler plugin with the **ingest** URL (uploads go to the ingest listener, not the admin one — same host as your DSN):

```bash
export SENTRY_URL=https://errors.example.com   # ingest host
export SENTRY_AUTH_TOKEN=spk_...                # project API key
export SENTRY_ORG=default                       # any value works
export SENTRY_PROJECT=1                         # project ID

sentry-cli sourcemaps upload ./dist
```

Bundler plugins (`@sentry/vite-plugin`, `@sentry/webpack-plugin`, etc.) accept the same environment variables, or you can pass them as options. Source maps are matched by debug ID and applied automatically when rendering stack traces. Stale upload chunks (and source maps themselves, per `retention_days`) are cleaned up by a background task.

## Monitors

Cron job monitoring is supported via Sentry's check-in protocol. SDKs send check-in envelopes with a monitor slug, and stackpit tracks their status (OK, error, in-progress) over time.

Browse monitors per-project at `/web/projects/{id}/monitors/` to see check-in history and current state.

## Web UI

The admin UI is server-rendered HTML, no SPA. Beyond Issues and Events, each project surfaces views for **transactions, logs, spans/traces, metrics, profiles, replays, release health, monitors, user reports, and client reports** — these reflect whatever the SDK sent in the corresponding envelope items. Performance rollups (transaction percentiles, throughput, failure rate), span waterfalls, Web Vitals, and release-health crash-free rates are all derived automatically from the incoming envelopes — there's nothing to configure. Most of it is read-only browsing; the things you'd actually configure (filters, integrations, alerts, source map keys, project archival) live under each project's **Settings**.

## Syncing from Sentry

If you've got historical data in an existing Sentry instance, you can pull it in with the `sync` command. It fetches events, issue statuses, attachments, and releases.

```bash
export SENTRY_AUTH_TOKEN=<your-api-token>

stackpit sync \
  --org my-org \
  --url https://sentry.io \
  --projects web-frontend,api-server
```

| Flag | Default | Description |
|------|---------|-------------|
| `--org` | required | Sentry organization slug |
| `--url` | `https://sentry.io` | Sentry API base URL (for self-hosted) |
| `--projects` | all | Comma-separated project slugs to sync |
| `--max-pages` | unlimited | Limit pages fetched per project |

Sync is resumable — it tracks watermarks and cursors, so you can re-run it to pick up new events without starting over.

## CLI tools

```bash
stackpit status                 # show environment & config overview (handy for debugging auth/OIDC config)
stackpit projects               # list known projects
stackpit events                 # list recent events
stackpit events -p 1 -l 50      # filter by project, set limit
stackpit event <event-id>       # show full event JSON
stackpit tail                   # stream new events in real-time
stackpit backfill-issues        # regenerate fingerprints & issue grouping
```

## Edge cases

**Issue grouping after sync:** stackpit uses its own fingerprinting (exception type+value, message template, SDK-provided fingerprint) which covers most cases but isn't identical to Sentry's server-side grouping — Sentry has additional heuristics like stack trace similarity. After syncing, new locally-received events will generally group into the correct existing issues, but exceptions where Sentry would split or merge based on stack frames may end up grouped slightly differently.

**Issue status sync requires events first:** When syncing issue statuses, stackpit matches by Sentry's group ID — which is only populated after events have been synced. If you sync statuses before events, status updates for unmatched issues are silently skipped. Always sync events first.
