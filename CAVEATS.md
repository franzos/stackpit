# Caveats

Deliberate trade-offs and non-obvious constraints. These are intentional, not bugs.

## Two JWT libraries, two JWKS parses

stackpit depends on **both** `openidconnect` and `jsonwebtoken`, and parses the
provider's JWKS into two in-memory forms. This is on purpose.

The two crates are independent crypto stacks: `openidconnect` ships its own JOSE
implementation on RustCrypto (`rsa`/`p256`/`sha2`/…), while `jsonwebtoken` is
built on `ring`/`aws-lc-rs`. They share no key type, so the same JWKS document
has to be parsed once per library. There is no single key type that works for
both, and `openidconnect` is its own crypto island regardless of what else is in
the tree.

We carry both because stackpit does three JWT jobs with conflicting rules:

- **id_token** (login) — verified by `openidconnect`, which enforces the strict
  id_token checks (nonce, `aud == client_id`). That strictness is exactly what
  we want at login.
- **MCP access token** (RS256, resource-server) — not an id_token; needs a
  relaxed, claims-agnostic RS256 verify.
- **back-channel `logout_token`** — the spec *forbids* nonce and aud-match,
  which `openidconnect`'s id_token verifier requires.

`openidconnect` does not publicly expose a generic JWS verify (the primitive
exists internally but is private), so the latter two go through `jsonwebtoken`.
Collapsing to one library isn't worth it: openidconnect-only would mean
hand-rolling JWT envelope parsing against a private primitive, and
jsonwebtoken-only would mean hand-rolling id_token verification — the most
security-critical, easiest-to-botch part of OIDC.

The runtime cost is small: `JwksCache` fetches the raw JWKS once (and refetches
on `kid` miss); only the in-memory parse is duplicated. A login-only OIDC app
(e.g. one built on `axum-oidc`) never hits this — it needs only `openidconnect`.
The second library shows up here precisely because stackpit is *also* a resource
server (MCP) and handles back-channel logout.

Call sites: `src/oauth.rs` (id_token), `stackpit-auth/src/jwks.rs` (MCP RS256),
`src/oidc/logout.rs` (logout_token).
