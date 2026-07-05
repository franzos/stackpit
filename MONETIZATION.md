# Monetization Notes

Working sketch of how Stackpit is licensed and what could plausibly be sold on
top of the open-source core. Not a commitment to any specific direction: today
the commercial machinery is pure infrastructure and gates nothing.

## Licensing model

**Open-core + dual-license.** The core (everything outside `src/commercial/`)
is MIT: freely usable, including commercially, with no strings. The files under
`src/commercial/` are source-available under the Stackpit Commercial License 1.0
(`LICENSE-COMMERCIAL`, SPDX `LicenseRef-Stackpit-Commercial-1.0`). A future
"Business" tier, sold per-instance with a signed license key, would unlock
gated features; the key is verified fully offline against a public key baked in
at build time.

- **No phone-home.** Verification runs locally against `src/commercial/pubkey.bin`.
  Nothing about a license is ever sent over the network.
- **No host binding.** There is no machine fingerprint or activation server.
- **No seat counting.** Stackpit carries a `max_orgs` cap (future use) but does
  not count seats. The issuer may still emit a `max_seats` flag; Stackpit ignores it.
- **Anti-leak is the signed watermark, not DRM.** Every license carries the
  customer name, contact email, and license id inside the signed claims. Those
  are surfaced on the `/web/admin/license` page and in the boot logs. Because
  they live inside the Ed25519 signature they cannot be edited without
  invalidating the blob: a leaked key advertises exactly who leaked it. It is a
  tripwire and a contractual deterrent, not a technical lock.

## The MIT/AGPL asymmetry (be honest about it)

Sister project Forseti pairs its commercial module with an **AGPL** core, so a
licensee who forks and strips a gate still owes source disclosure the moment
they run the fork as a network service. Stackpit's core is **MIT**, which gives
no copyleft backstop. Once a hypothetical feature is gated by a
`state.license.feature(...)` call sitting in ordinary MIT code, a licensee can
fork, delete that one call site, and keep the fork private with zero disclosure
obligation. The Commercial License only binds someone who actually invokes the
code under `src/commercial/`.

So for Stackpit the watermark plus the contract are the whole deterrent; there
is no legal copyleft leverage behind them. That is a deliberate trade for a
permissive core, and it is worth stating plainly rather than pretending the
gate is load-bearing.

## Honesty caveat: what activation does today

Nothing functional. Activating a license currently changes a status label on
the (unlinked) `/web/admin/license` page and writes the signed watermark to the
boot log. No behaviour is gated. This is the infrastructure (offline verifier,
singleton persistence, runtime handle, admin page) put in place so that a future
feature is just a new `Feature` variant in `src/commercial/license.rs` plus one
gate call at the site. Until such a feature exists, the OSS build and a licensed
build are functionally identical.

## Gated-feature matrix

TBD. The infrastructure is in place; no features are gated yet. The `Feature`
enum in `src/commercial/license.rs` is intentionally empty, and there is a
`max_orgs` quantitative cap dimension (`org_cap_allows`) wired but unused. When
a feature is chosen it slots in as: one enum variant (wire name + label), one
`state.license.feature(Feature::X)` check at the call site, and the issuer signs
the feature string into the blob.

## Where a first paid capability might come from

Cheapest-first, reusing infra that already exists in the tree:

- **Quantitative caps.** The `max_orgs` + `org_cap_allows()` pattern ("count
  rows, check the cap at create-time, else block") is already built and applies
  verbatim to projects, API keys, retention windows. Turns any resource into an
  "unlimited X" upsell for near-zero net-new code.
- **Long event / trace retention + export.** Retention already keys on a config
  value; OSS caps it, commercial unlocks longer windows plus an export button.
- **Notification / alerting fan-out.** The notify dispatcher and integrations
  surface already exist; gating advanced destinations is a call-site check.

None of this is committed. The point of Phase B is only that adding any of it
later is a small, well-scoped change rather than a rebuild.
