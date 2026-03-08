<p align="center">
  <img src="assets/logo.svg" alt="stackpit" width="480">
</p>
<p align="center">
  A drop-in, self-hosted replacement for Sentry's event ingestion and browsing. Single binary, single SQLite file, no external dependencies.
</p>

I got tired of paying for Sentry on smaller projects and self-hosting the official thing is... a lot. The thing is, most of what I need is ingestion, grouping, and a way to browse errors. So I built this — point your existing Sentry SDKs at it, browse errors in the web UI, or query via the JSON API.

## Install

| Method | Command |
|--------|---------|
| Cargo | `cargo install stackpit` |
| Homebrew | `brew tap franzos/tap && brew install stackpit` |
| Debian/Ubuntu | Download [`.deb`](https://github.com/franzos/stackpit/releases) — `sudo dpkg -i stackpit_*_amd64.deb` |
| Fedora/RHEL | Download [`.rpm`](https://github.com/franzos/stackpit/releases) — `sudo rpm -i stackpit-*.x86_64.rpm` |
| Guix | `guix install -L <panther> stackpit` ([Panther channel](https://github.com/franzos/panther)) |

Pre-built binaries for Linux (x86_64), macOS (Apple Silicon, Intel) on [GitHub Releases](https://github.com/franzos/stackpit/releases).

## Running

```bash
stackpit init            # create default stackpit.toml
stackpit serve           # start both ingestion + admin UI
stackpit serve --ingest-only  # ingestion only, no admin UI/API
```

### Ports

stackpit runs two listeners:

| Port | Default | Purpose |
|------|---------|---------|
| Admin | `127.0.0.1:3000` | Web UI + JSON API (localhost only) |
| Ingestion | `0.0.0.0:3001` | Receives SDK traffic (all interfaces) |

The admin port serves the browsing UI and API. The ingestion port is where your SDKs send events — it's the address you put in your DSN. I've found that keeping these separate makes deployment quite a bit more flexible.

`--ingest-only` skips the admin listener entirely, useful if you want dedicated ingestion nodes.

## Configuration

Config lives in `stackpit.toml` (override with `-c /path/to/config.toml`).

```toml
[server]
bind = "127.0.0.1:3000"       # admin UI/API address
ingest_bind = "0.0.0.0:3001"  # SDK ingestion address
external_url = ""              # external URL for DSN generation (optional)
admin_token = ""               # shared bearer token for admin auth (optional)
max_body_size = 10485760       # max decompressed body in bytes (default 10MB)
max_compressed_body_size = 0   # max compressed body in bytes (default max_body_size / 5)

[storage]
path = "stackpit.db"           # SQLite database path
database_url = ""              # full URL, e.g. "postgres://user:pass@host/stackpit" (overrides path)
retention_days = 90            # auto-delete events older than this (0 = keep forever)

[filter]
mode = "open"                  # "open" = allow all, "closed" = deny all
rate_limit = 0                 # global max events per minute (0 = unlimited)
max_projects = 1000            # max auto-registered projects in open mode
excluded_environments = []     # environment names to reject globally
blocked_user_agents = []       # user-agent glob patterns to block globally

[notifications]
rate_limit_per_project = 30    # max notifications per project per 60s (0 = unlimited)
rate_limit_global = 100        # max total notifications per 60s (0 = unlimited)
```

**Filter modes:** `open` accepts everything unless it matches a deny rule. `closed` rejects everything unless it matches an allow rule.

Per-project filter rules are managed in the web UI under **Filters**. Available rule types include message patterns (glob/regex), CIDR/IP blocks, release and environment exclusions, and fingerprint discards. Filtering runs in three tiers — fingerprint discard and inbound filters first, then rate limits and user-agent blocks, then IP rules — so cheap checks happen before expensive ones.

All fields have sane defaults. An empty config file works fine.

### PostgreSQL

SQLite is the default, but you can point stackpit at a PostgreSQL database instead:

```toml
[storage]
database_url = "postgres://user:pass@localhost/stackpit"
```

When `database_url` is set it takes precedence over `path`. Migrations run automatically on startup for both backends.

### Authentication

Set `admin_token` in the config to require a bearer token for the admin UI and API. When set, requests need either an `Authorization: Bearer <token>` header or a `stackpit_token` cookie (set via the login page at `/web/login`).

### Secret encryption

Integration credentials (Slack tokens, webhook URLs, etc.) can be encrypted at rest. Set a 32-byte hex key:

```bash
export STACKPIT_MASTER_KEY=$(openssl rand -hex 32)
```

Without this, secrets are stored in plaintext. stackpit warns on startup when the key is missing.

## Notifications & Alerts

stackpit can notify you when things go wrong. Integrations (email via Postmark, Slack, webhooks) are configured in the web UI under **Settings → Integrations**, and each project can enable or disable specific triggers.

**Immediate notifications** fire during event ingestion:

- **New issue** — a fingerprint appears for the first time
- **Regression** — a previously resolved issue reappears
- **Threshold exceeded** — a custom alert rule fires (e.g. 100 events in 5 minutes)

**Digest emails** summarize activity over a configurable interval — new issues, active issue counts, and total events per project. Digest schedules can be per-project or global.

Each project integration can filter notifications by trigger type, minimum severity level, and environment. Rate limiting (configurable in `[notifications]`) prevents notification storms.

Alert rules and digest schedules are managed via the web UI under **Alerts**, or through the JSON API (`/api/v1/alerts/rules`, `/api/v1/digests`).

## Source Maps

stackpit supports source map uploads so JavaScript stack traces resolve to original source locations. Upload artifact bundles using the standard Sentry CLI:

```bash
sentry-cli sourcemaps upload --org my-org --project my-project ./dist
```

The upload endpoints (`/api/0/organizations/{org}/chunk-upload/` and `.../artifactbundle/assemble/`) are Sentry-compatible. Source maps are matched by debug ID and applied automatically when rendering stack traces in the web UI.

Stale upload chunks older than 24 hours are cleaned up by a background task.

## Monitors

Cron job monitoring is supported via Sentry's check-in protocol. SDKs send check-in envelopes with a monitor slug, and stackpit tracks their status (OK, error, in-progress) over time.

Browse monitors per-project at `/web/projects/{id}/monitors/` to see check-in history and current state.

## Supported SDKs

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
stackpit status                 # show environment & config overview
stackpit projects               # list known projects
stackpit events                 # list recent events
stackpit events -p 1 -l 50     # filter by project, set limit
stackpit event <event-id>       # show full event JSON
stackpit tail                   # stream new events in real-time
stackpit backfill-issues        # regenerate fingerprints & issue grouping
```

## Edge cases

**Issue grouping after sync:** stackpit uses its own fingerprinting (exception type+value, message template, SDK-provided fingerprint) which covers most cases but isn't identical to Sentry's server-side grouping — Sentry has additional heuristics like stack trace similarity. After syncing, new locally-received events will generally group into the correct existing issues, but exceptions where Sentry would split or merge based on stack frames may end up grouped slightly differently.

**Issue status sync requires events first:** When syncing issue statuses, stackpit matches by Sentry's group ID — which is only populated after events have been synced. If you sync statuses before events, status updates for unmatched issues are silently skipped. Always sync events first.

## Acknowledgements

This project wouldn't be possible without [Sentry](https://sentry.io) and is not meant to be a replacement, but rather a lightweight drop-in with limited features. If you need the full power of Sentry — performance monitoring, session replay, profiling, and so on — use the real thing.

## Building

Requires Rust 1.88+.

```bash
cargo build --release
```
