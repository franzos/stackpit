# Stackpit

Stackpit is a drop-in, self-hosted replacement for Sentry's event ingestion and browsing: a single binary backed by a single SQLite file, with no external dependencies. Point your existing Sentry SDKs at it, browse errors in the web UI, or query them over the JSON API.

These docs are split by what you're here to do:

- [User guide](./user-guide.md) — for people browsing and triaging errors in the web UI.
- [Operator guide](./operator-guide.md) — the full `stackpit.toml` reference, PostgreSQL, authentication and OIDC/SSO, connecting SDKs, notifications, source maps, monitors, syncing from Sentry, and the CLI.
- [Commercial features](./commercial/index.md) — the MIT-core / commercial-gate split and observability.

The source lives on [GitHub](https://github.com/franzos/stackpit). Stackpit is MIT with a commercial option.
