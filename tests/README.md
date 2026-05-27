# Tests

Two suites beyond the inline unit tests (`#[cfg(test)]` modules in `src/`).
Both drive a running, seeded `stackpit serve`; neither is part of `make test`
(unit-only) or CI (the integration target is gated behind the `integration-tests`
cargo feature, so a bare `cargo test` skips it).

`make` isn't on the GUIX PATH here — invoke targets via `guix shell make -- make <target>`.

## Lifecycle

`make serve-bg` wipes `stackpit.db`, launches the server (admin :3333, ingest
:3334), and waits for health. Then `make seed` ONCE — you cannot reseed a
running server (the seeder mints fresh random keys each run and open-mode pins
the first DSN, so a second seed is rejected). To start over: `serve-bg` (wipes)
then `seed` again. `serve-bg` auto-generates an ephemeral `STACKPIT_MASTER_KEY`
if none is exported.

## Rust integration (`tests/integration/`)

Drives the running server over HTTP via `reqwest` — no browser. The round-trip
test shells out to the `sqlite3` CLI, so include it in the shell:

```sh
guix shell make -- make serve-bg
guix shell make -- make seed
guix shell make sqlite -- make test-integration
```

Runs with `--test-threads=1` (shared server + per-IP rate-limit buckets). The
admin login is capped at 10/min/IP and the suite logs in a few times per run,
so very rapid repeated runs can briefly 429 — wait ~60s.

Config: URLs and the admin token are read from `stackpit.toml` (repo root), or
overridden via `STACKPIT_ADMIN_URL` / `STACKPIT_INGEST_URL` / `STACKPIT_ADMIN_TOKEN`.

## Playwright (`tests/e2e/`)

A thin admin-UI smoke suite (5 specs + a login-once setup step), run in
Microsoft's Playwright podman image:

```sh
guix shell make -- make serve-bg && guix shell make -- make seed
guix shell make -- make e2e          # 5 smoke specs (+ setup)
guix shell make -- make e2e-trace    # open the HTML report from the last run
```
