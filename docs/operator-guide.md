# Operator Guide

Everything beyond getting the binary and starting it: the full configuration reference, authentication and OIDC setup, connecting SDKs, notifications, source maps, syncing, and the CLI. For install and first boot, see the [README](../README.md).

## Contents

- [Configuration](#configuration)
- [PostgreSQL](#postgresql)
- [Authentication](#authentication)
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
  --scope "openid email profile offline_access" \
  --token-endpoint-auth-method client_secret_post \
  --redirect-uri https://stackpit.example.com/web/auth/callback
```

Then wire it into `stackpit.toml`:

```toml
[server]
external_url = "https://stackpit.example.com"   # required for OAuth — callback URLs are built from this

[auth.oauth]
issuer_url    = "https://hydra.example.com"
client_id     = "<from hydra output>"
client_secret = "<from hydra output>"
redirect_uri  = "https://stackpit.example.com/web/auth/callback"
web_audience  = "stackpit-web"                  # required — must match the IdP audience for the web client
```

`web_audience` binds the BFF to the audience your IdP issues to the web client; it blocks confused-deputy attacks across resource servers and is enforced at startup.

User rows are provisioned just-in-time on first login, linked by the OIDC `(iss, sub)` pair. Every authenticated user sees everything — there is no admin/user privilege split in the UI today. The `admin_token` is a separate break-glass code path (CLI, headless ops) and doesn't flow through the users table at all. Email is stored only when the IdP reports `email_verified=true`; unverified emails are ignored to keep an attacker-controlled string out of identity decisions.

A "Sign in with SSO" button appears on `/web/login` whenever `[auth.oauth]` is configured. The admin_token path keeps working alongside as a break-glass.

The full set of OAuth knobs (`post_logout_redirect_uri`, `access_token_max_ttl_secs`, `introspection_cache_ttl_secs`, etc.) is documented inline in the config that `stackpit init` writes — read that file for the authoritative reference.

## Secret encryption

When OAuth is enabled, stackpit **requires** a 32-byte hex master key — OIDC tokens are stored encrypted at rest in `oidc_grants` and startup will refuse without it:

```bash
export STACKPIT_MASTER_KEY=$(openssl rand -hex 32)
```

The same key encrypts integration credentials (Slack tokens, webhook URLs). Without it and without OAuth, integration secrets fall back to plaintext storage and stackpit warns on startup.

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

stackpit can notify you when things go wrong. Integrations (email, Slack, webhooks) are configured in the web UI under **Settings → Integrations**, and each project can enable or disable specific triggers.

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

The admin UI is server-rendered HTML, no SPA. Beyond Issues and Events, each project surfaces views for **transactions, logs, spans/traces, metrics, profiles, replays, release health, monitors, user reports, and client reports** — these reflect whatever the SDK sent in the corresponding envelope items. Most of it is read-only browsing; the things you'd actually configure (filters, integrations, alerts, source map keys, project archival) live under each project's **Settings**.

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
