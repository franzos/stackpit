<p align="center">
  <img src="assets/logo.svg" alt="stackpit" width="480">
</p>
<p align="center">
  A drop-in, self-hosted replacement for Sentry's event ingestion and browsing. Single binary, single SQLite file, no external dependencies.
</p>
<p align="center">
  <a href="https://github.com/franzos/stackpit/actions/workflows/ci.yml"><img src="https://github.com/franzos/stackpit/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/franzos/stackpit/actions/workflows/release.yml"><img src="https://github.com/franzos/stackpit/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="https://github.com/franzos/stackpit/pkgs/container/stackpit"><img src="https://img.shields.io/badge/ghcr.io-stackpit-097aba?logo=docker&logoColor=white" alt="Container"></a>
</p>

I got tired of paying for Sentry on smaller projects and self-hosting the official thing is... a lot. The thing is, most of what I need is ingestion, grouping, and a way to browse errors. So I built this — point your existing Sentry SDKs at it, browse errors in the web UI, or query via the JSON API.

## Features

- **Drop-in Sentry protocol** — envelope and legacy store endpoints, all auth methods. Any Sentry SDK works, no code changes.
- **Single binary, no dependencies** — one process, one SQLite file. PostgreSQL optional.
- **Issue grouping** — fingerprint-based grouping with regressions and resolution tracking.
- **Server-rendered web UI** — browse issues, events, transactions, logs, traces, replays, monitors, and more.
- **JSON API** — query everything the UI shows.
- **Notifications & alerts** — email (Lettermint, Postmark, or SendGrid), Slack, and webhooks, with digests and threshold rules.
- **Source maps** — upload via `sentry-cli` so minified traces resolve to original source.
- **Monitors** — cron check-in tracking via Sentry's protocol.
- **Auth your way** — a shared admin token for solo use, or OAuth/OIDC SSO for teams.
- **Migrate in** — pull historical events, issues, and releases from an existing Sentry instance.

## Install

| Method | Command |
|--------|---------|
| Cargo | `cargo install stackpit` |
| Homebrew | `brew tap franzos/tap && brew install stackpit` |
| Debian/Ubuntu | Download [`.deb`](https://github.com/franzos/stackpit/releases) — `sudo dpkg -i stackpit_*_amd64.deb` |
| Fedora/RHEL | Download [`.rpm`](https://github.com/franzos/stackpit/releases) — `sudo rpm -i stackpit-*.x86_64.rpm` |
| Guix | `guix install -L <panther> stackpit` ([Panther channel](https://github.com/franzos/panther)) |
| Docker | `docker pull ghcr.io/franzos/stackpit:latest` ([all tags](https://github.com/franzos/stackpit/pkgs/container/stackpit)) |

Pre-built binaries for Linux (x86_64) and macOS (Apple Silicon, Intel) on [GitHub Releases](https://github.com/franzos/stackpit/releases).

## Running

```bash
stackpit init            # writes stackpit.toml with a fresh admin_token
stackpit serve           # start both ingestion + admin UI
stackpit serve --ingest-only  # ingestion only, no admin UI/API
```

`stackpit init` generates a random 32-byte admin token and writes it into the config, so the admin UI is usable on first boot without any extra steps.

### Docker

Images are published to the GitHub Container Registry on every release — a default **SQLite** image and a **PostgreSQL** variant (same tags, `-postgres` suffix):

```bash
docker pull ghcr.io/franzos/stackpit:latest            # SQLite
docker pull ghcr.io/franzos/stackpit:latest-postgres   # PostgreSQL

# first run: generate stackpit.toml (with an admin token) into the volume
docker run --rm -v stackpit-data:/app ghcr.io/franzos/stackpit:latest ./stackpit init

# then serve
docker run -d --name stackpit \
  -p 3000:3000 -p 3001:3001 \
  -v stackpit-data:/app \
  ghcr.io/franzos/stackpit:latest
```

The SQLite file and `stackpit.toml` live in the working directory (`/app`) — mount a volume there to persist them. Note the admin listener binds to `127.0.0.1` by default, so set `bind = "0.0.0.0:3000"` in `stackpit.toml` for the mapped `3000` port to be reachable from outside the container.

### Ports

stackpit runs two listeners:

| Port | Default | Purpose |
|------|---------|---------|
| Admin | `127.0.0.1:3000` | Web UI + JSON API (localhost only) |
| Ingestion | `0.0.0.0:3001` | Receives SDK traffic (all interfaces) |

The admin port serves the browsing UI and API. The ingestion port is where your SDKs send events — it's the address you put in your DSN. I've found that keeping these separate makes deployment quite a bit more flexible.

`--ingest-only` skips the admin listener entirely, useful if you want dedicated ingestion nodes.

## Documentation

Everything past first boot — the full `stackpit.toml` reference, PostgreSQL, authentication and OIDC/SSO setup, connecting SDKs, notifications, source maps, monitors, syncing from Sentry, and the CLI — lives in the **[Operator Guide](docs/operator-guide.md)**:

- [Configuration](docs/operator-guide.md#configuration) — the full config reference, filter modes, [PostgreSQL](docs/operator-guide.md#postgresql)
- [Authentication](docs/operator-guide.md#authentication) — admin token, OAuth/SSO (OIDC), [secret encryption](docs/operator-guide.md#secret-encryption)
- [Connecting SDKs](docs/operator-guide.md#connecting-sdks) — supported SDKs and DSN format
- [Notifications & Alerts](docs/operator-guide.md#notifications--alerts), [Source Maps](docs/operator-guide.md#source-maps), [Monitors](docs/operator-guide.md#monitors), [Web UI](docs/operator-guide.md#web-ui)
- [Syncing from Sentry](docs/operator-guide.md#syncing-from-sentry), [CLI tools](docs/operator-guide.md#cli-tools)

## Acknowledgements

This project wouldn't be possible without [Sentry](https://sentry.io) and is not meant to be a replacement, but rather a lightweight drop-in with limited features. If you need the full power of Sentry — performance monitoring, session replay, profiling, and so on — use the real thing.

## Building

Requires Rust 1.88+.

```bash
cargo build --release
```
