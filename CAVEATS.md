# Caveats

Deliberate trade-offs and non-obvious constraints. These are intentional, not bugs.

## One JWKS parse, two JWT libraries underneath

Every IdP-signed token goes through the `oidc-relying-party` crate: ID tokens (`src/oidc/client.rs`), MCP access tokens (`stackpit-auth/src/bearer/jwt.rs`) and back-channel logout tokens (`src/oidc/logout.rs`) share the issuer's `JwksCache` (an `auth.mcp.jwks_url` override gets its own), which parses each JWKS once and applies the same key rules to all three. The crate verifies ID tokens with `openidconnect` and the other two with `jsonwebtoken`, so both stay in the dependency tree, neither as a runtime dependency of Stackpit's own crates; only the one matched key is converted to `openidconnect`'s type, per ID-token verification.

## A second copy of the refresh token for back-channel logout

`oidc_grants` encrypts every token with the browser's raw cookie handle as AAD and stores only its hash, so the database plus the master key cannot read a grant's tokens. Back-Channel Logout 1.0 §2.7 wants the refresh tokens of session-bound grants (no `offline_access`) revoked when a logout token ends them, and that handler never sees a cookie. `refresh_token_bc` is the same refresh token encrypted under AAD = the hashed handle, written wherever the refresh token is written and only for grants without `offline_access`, read only by the back-channel handler. That copy is readable with the database and the master key; the access and ID tokens keep the stronger property. The `offline` flag comes from the token response's granted `scope`, updated on a refresh only when the response lists `scope` again (RFC 6749 §5.1 lets the OP omit it); an OP that never returns `scope` leaves every grant marked session-bound.
