//! Integration test entry point.
//!
//! These tests drive a running `stackpit serve` over HTTP via `reqwest` — no
//! browser. They are NOT part of `cargo test`'s default run; invoke via
//! `make test-integration` after `make serve-bg && make seed`.
//!
//! `--test-threads=1` is required: the tests share one server (and its per-IP
//! rate-limit buckets). Cross-run isolation comes from a wiped+seeded DB per
//! `make serve-bg` plus per-run unique project IDs, not per-test cleanup.

#[path = "integration/common.rs"]
mod common;

#[path = "integration/smoke.rs"]
mod smoke;

#[path = "integration/auth.rs"]
mod auth;

#[path = "integration/ingest.rs"]
mod ingest;

#[path = "integration/api.rs"]
mod api;

#[path = "integration/security.rs"]
mod security;

#[path = "integration/orgs.rs"]
mod orgs;
