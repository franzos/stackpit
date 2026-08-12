# MCP: current state

Working notes as of 2026-08-04. Spans three repos: `~/git/stackpit`, `~/git/forseti`, `~/git/infrastructure`. Untracked on purpose.

Plan and test guide live in brain: `~/git/brain/projects/stackpit/plans/2026-08-03-mcp-e2e.md`, `~/git/brain/projects/stackpit/tests/mcp-e2e.md`.

## Where things stand

Stackpit has a working MCP endpoint, proven end to end against the local Forseti/Hydra stack. Two patch-level defects are fixed but uncommitted. Forseti has a critical audience defect fixed but uncommitted. Production has never had MCP enabled and none of the infrastructure changes have been applied.

The open design question — how resource servers get registered without an admin editing config — is **not solved**. That is the main thing outstanding.

## What shipped (tagged and public)

**stackpit v0.3.24** (`fe28681`, tag pushed, GitHub release green)
- MCP endpoint at `POST /mcp`: Streamable HTTP, legacy handshake era, versions `2024-11-05` through `2025-11-25`, stateless.
- 17 tools across read / write / admin scope tiers, all through one authorization choke point (`mcp::principal::authorize_tool`).
- RFC 9728 metadata, path-aware plus root alias, CORS-public.
- Per-tool scope enforcement with HTTP 403 + `WWW-Authenticate` step-up.
- Org membership refreshed from the IdP per request (userinfo → `orgs::reconcile`), cached 60s on token hash.
- Fixed in passing: a pre-existing cross-org leak where `spans::get_trace_spans` had no project filter, so a known trace id exposed another org's spans through the web UI.

**forseti v0.1.23** (tag pushed, CI green)
- `[oauth].allowed_resource_audiences`: bridges RFC 8707 `resource=` into Hydra's granted audience, since Hydra ignores `resource` entirely.
- DCR proxy injected the allow-list into every registered client so the refresh grant would pass.

**Both tags carry known defects.** See "Problems" below.

## What is uncommitted

**stackpit**
- `stackpit-auth/src/bearer/jwt.rs` — reject a JWT with no `aud` claim at all (`set_required_spec_claims`), enable `nbf` validation.
- `src/mcp/mod.rs` — add `openid` to `scopes_supported`.
- `src/trackers.rs` — seed org 2 in a test; Postgres enforces the FK, SQLite does not. This was failing CI on `v0.3.24`.
- `tests/integration/security.rs` — pre-existing failure, unrelated: asserted `icon.svg` while the app has served `icon.png` since the favicon commit.

**forseti** — the whole P0 audience fix
- `resolve_granted_audience` in `src/oauth/consent.rs`: one chokepoint, unions both audience carriers, default deny.
- `operator_written_audiences`: the client's Hydra record is policy only when `oauth_client_metadata.source = 'admin'`.
- DCR proxy strips caller-posted `audience` and no longer returns RFC 7592 credentials.
- `inject_allowed_resource_audiences` deleted; replaced by a targeted `add /audience/-` patch at consent time.
- Tests and operator docs rewritten.

**infrastructure** — never applied
- haproxy: `/.well-known/oauth-protected-resource` added to the Stackpit admin ACL.
- `stackpit.toml.j2`: `[auth.mcp]` with audience and `allowed_origins`.
- Forseti `config.toml.j2`: `allowed_resource_audiences` plus scope descriptions for the consent screen.

## What I looked into

- **MCP spec.** Current revision is `2026-07-28` (stateless, per-request `_meta`, mandatory `server/discover`). Claude Code 2.1.220 speaks only the legacy handshake era, verified by scanning the binary for version strings. Built for legacy, left a comment where the modern era slots in.
- **Hydra/fosite audience mechanics.** Hydra ignores RFC 8707 `resource` and binds `aud` only from its non-standard `audience=`. fosite re-validates the granted audience against the client record on the **refresh** grant but not the code exchange — which is why the DCR injection existed. fosite's resource-indicators PR (ory/fosite#879) was closed unmerged on 2026-08-04.
- **Two adversarial reviews plus a rebuttal round** on how to gate resource→audience at scale. Both reviewers changed position. Converged on: registry keyed to org domain-ownership proof, per-resource client-eligibility policy, RFC 9728 fetch demoted to enrolment-time corroboration.
- **IdP portability audit** against Keycloak, Auth0, Okta, Entra, Authentik, Zitadel, node-oidc-provider, Dex.
- **`github.com/ory/mcp`** — TypeScript glue implementing the MCP SDK's provider interface against Ory Network/Hydra. Each MCP server holds a Hydra admin key and registers clients itself; validates by introspection. No RFC 8707, no RFC 9728, no audience binding.
- **Ory's own MCP + Hydra guide** — no `resource`, no `audience=`, no `aud` validation at all; scope-only authorization, anonymous DCR straight at Hydra, single MCP server assumed. Their reference architecture never encounters this problem, which is why there's no upstream fix.

### Reproduced live (not inferred)

| Attack / behaviour | Result before | Result after |
| --- | --- | --- |
| DCR client posts its own `audience`, uses `audience=` | Working token for real Stackpit MCP, tool call 200 | Audience stripped at proxy |
| DCR client posts nothing; injected allow-list + `audience=` | Working token + refresh | Injection removed |
| RFC 7592 `PUT` self-writes the audience | Working token, tool call 200 | Record not trusted → `aud: []` |
| Registers directly at Hydra (bypasses Forseti) | Self-declared audience honoured | No metadata row → `aud: []` |
| JWT with **no** `aud` claim | Accepted by Stackpit | Rejected |
| Only the advertised scopes (no `openid`) | Works on Hydra | Would break on Keycloak/Okta; `openid` now advertised |

## Problems

### 1. `allowed_resource_audiences` does not scale — the main open issue

Every MCP server must be typed into Forseti's config, by an admin, with a restart. MCP clients are DCR-registered, so the client-record arm never applies to them: **the allow-list is the only path by which an MCP client can ever obtain an audience.** Remove Stackpit's entry and MCP stops working.

This is the thing Franz has objected to repeatedly and it is still true.

### 2. The allow-list is a ceiling, not authorization

Any client — including one registered anonymously seconds earlier — can request any *listed* audience and get it, given a user who clicks Allow. The list answers "which resources may exist as audiences", never "which client may target this resource". No allow-list design can close that.

Damage is currently limited downstream: Stackpit intersects token scopes with the user's DB role, so there is no privilege escalation. That is Stackpit's discipline covering Forseti's gap, not a property of the AS.

### 3. Deploy blocker for the Forseti fix

The record arm trusts a client only when `oauth_client_metadata.source = 'admin'`. Clients created out of band (`hydra create client`, admin API directly, the `forseti:link` skill) have **no row**, so their audience stops being trusted and their SSO breaks on upgrade.

Not hypothetical: the local Stackpit web client `22674ee0-…` has no row. The local DB holds 58 `admin` and 236 `dcr` rows with it in neither. Production's table has not been checked.

Remedies, both verified locally: adopt the client in Forseti's admin UI, or list its audience (the list now matches verbatim before canonicalising, so non-URI identifiers like `stackpit.gofranz.com` work).

### 4. IdP portability — Stackpit assumes Hydra in several places

Fixed: `scp` vs `scope`, absent `aud`, `nbf`, missing `openid`.

Outstanding, blocking for any non-Hydra IdP:
- `McpPrincipal` hard-requires a `userinfo_endpoint` and fails closed. It's only RECOMMENDED in OIDC Discovery; Entra points it at Graph (401 → re-auth loop); Keycloak and node-oidc-provider can return `application/jwt`, which `resp.json()` can't parse. MCP becomes entirely unusable, `whoami` included.
- RS256 is hard-pinned; a valid ES256/PS256 token is rejected with a misleading signature error.
- `client_id` read from one spelling; Keycloak/Auth0/Entra emit `azp`, Okta `cid`.
- The opaque/introspection arm reads only `scope`, never `scp` — the same bug already paid for on the JWT side.

### 5. Documentation drift

`~/git/brain/notes/mcp-server-behind-forseti.md` presents the config allow-list as *the* mechanism. After today that is misleading for the next app; it needs rewriting around the source rule, the bypasses found, and the registry direction.

CHANGELOG entries for both repos deliberately not written.

### 6. Not verified

Production `oauth_client_metadata` contents. Claude Code and claude.ai against the real instance — MCP has never been enabled in production.

## What Franz criticized

Recorded so it isn't lost, and because most of it was right.

- **Editing `allowed_resource_audiences` in config, repeatedly.** "Isn't this a bit odd, shouldn't it dynamically support other MCP?" then "there may be 100s of MCPs using Forseti; we cannot manually maintain each" then "You're not being clear with me. What about allowed_resource_audiences? It still includes Stackpit and others." The objection stands and is unresolved — see Problem 1. Compounding it: I re-added an entry (`stackpit-web`) as a workaround for the deploy blocker and reported it alongside the design discussion, which made it look like I'd quietly re-adopted the manual list as the answer.
- **Advising `dcr_require_iat = true` as a stopgap.** Wrong. Anonymous DCR is by design — the deployed `hydra.yml` says so outright ("Required for MCP — Claude Code refuses any AS that doesn't expose `/oauth2/register`"). The advice would have broken the feature we'd just built to partially mitigate a bug that had a proper fix in flight. Withdrawn.
- **Proposing an haproxy change** to block `/hydra/oauth2/register`. Not wanted, and unnecessary: Hydra is the OAuth server and its endpoints being public is normal. The correct takeaway is narrower — Forseti's DCR-proxy gates (IAT, reserved names, rate limits) are hygiene, not security controls, and nothing may depend on them. That is now true by construction.
- **"So you're saying, leave everything as is?"** My reporting buried the active work under a list of things awaiting his decision, so it read as inaction when the P0 fix was mid-flight.
- **Unclear, fragmented communication generally.** Reporting per-agent findings as they arrived instead of maintaining one coherent picture. This file is the correction.

## My errors, for the record

- **The amendment instruction reopened the defect.** I told the fix agent "after stripping DCR audiences, a client can no longer write its own record." False — Hydra hands every DCR client an RFC 7592 token pointing straight at itself. The agent implemented it correctly; the instruction was wrong. Caught by my own follow-up test.
- **My proposed fallback would have reopened it a third time.** "No metadata row ⇒ trust the record, only an operator could have created it." The agent found the direct-to-Hydra registration path and declined to implement it. Correct call.
- **Two invalid tests that "passed".** One accepted consent via Hydra's admin API, bypassing the very resolver under test. One used an audience that was on the allow-list, so it was granted legitimately and looked like a bypass. Both would have produced false confidence.
- **Missed the Postgres CI failure** on the release I tagged, because I only ran `cargo check --features postgres`, which cannot catch an FK violation.
- Used `sed` to edit `Cargo.toml` instead of the file tools.

## Options for next

Not a decision, just the shape of the choices.

1. **Ship the fixes.** Two patch releases; closes three verified bypasses plus Stackpit's absent-`aud` hole. Requires checking production's client table first (Problem 3).
2. **Build the registry.** Resource servers enrol themselves via org domain-ownership proof; per-resource client-eligibility policy. Retires the manual list for MCP servers (Problem 1) and closes the phishing gap (Problem 2). This is the real answer to the criticism.
3. **Finish IdP portability** (Problem 4) before anyone points Stackpit at a non-Hydra IdP.
4. **Deploy and run production E2E** — the harness takes `STACKPIT`/`RESOURCE` env vars, so it's one command once `[auth.mcp]` is live.

Sequencing view: 1 and 2 are independent of each other; 2 is what makes the answer to "hundreds of MCP servers" stop being "edit the config".
