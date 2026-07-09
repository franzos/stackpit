# Commercial Features

Stackpit's core is everything you need to run a self-hosted, Sentry-compatible error tracker: ingestion, issue grouping, the web UI and JSON API, organizations, auth/OIDC, notifications, source maps, monitors, retention, all of it. A commercial license unlocks a small, separate set of features on top.

This page is the buyer/operator overview: what a license unlocks today, how the offline licensing model works, and where the free/paid line sits. For the operator detail of each feature, follow the links below.

## What a license unlocks today

Exactly one feature is gated so far:

- **[Observability](./observability.md)**: a Prometheus `/metrics` endpoint on the admin listener, token-gated, exposing HTTP RED metrics (request counts, latency) plus bridged ingestion counters. A stock Prometheus, Grafana Agent, or OTel Collector scrapes it as-is.

That's the whole list. Don't plan around anything else, there is nothing else shipped or gated yet.

## How licensing works

Licensing is **offline**. Stackpit never calls out to validate a license; verification runs entirely against a public key baked in at build time (`src/commercial/pubkey.bin`). There's no license server and no outbound call at runtime, which matters if you self-host in a network that can't (or won't) reach the internet.

A license is a small Ed25519-signed blob. You activate it by pasting it at **`/web/admin/license`**. Stackpit verifies the signature itself, reads the customer name, contact email, license id, expiry, and the list of enabled features, then stores the result in the database. Each feature is unlocked independently.

- **No phone-home.** Verification is local; nothing about a license is ever sent over the network.
- **No host binding.** There's no machine fingerprint or activation server tying a license to a specific box.
- **No seat counting.** Stackpit doesn't count seats. (A `max_orgs` cap exists in the license shape for future use, but nothing enforces it today.)
- **The deterrent is the signed watermark, not DRM.** The customer name, contact email, and license id live inside the signed claims, surfaced on the `/web/admin/license` page and in the boot logs. Because they're inside the Ed25519 signature, they can't be edited without invalidating the blob: a leaked key advertises exactly who leaked it. That's a tripwire and a contractual deterrent, not a technical lock.

### Active, grace, and locked

A license can carry an expiry (lifetime licenses never expire). Stackpit checks it against the clock at startup and whenever you activate one, putting each licensed feature into one of three states:

| State | What it means operationally |
|---|---|
| **Active** | Licensed and before expiry. The feature works normally. |
| **Grace** | Past expiry but still inside the grace period. The feature keeps working read-only: for Observability specifically, the `/metrics` endpoint keeps serving. |
| **Locked** | No license, the license doesn't include this feature, or the grace period has passed. The gated surface returns 404. |

The grace period is a fixed **30 days**, not operator-configurable. It's a safety net so a forgotten renewal doesn't silently break a monitoring dashboard the moment a license expires.

## The MIT-core-vs-commercial split

Stackpit is dual-licensed:

- The **core** (everything outside `src/commercial/`) is **MIT**: freely usable, including commercially, with no strings attached.
- The files under `src/commercial/` are source-available under the **Stackpit Commercial License 1.0** (`LICENSE-COMMERCIAL`).
