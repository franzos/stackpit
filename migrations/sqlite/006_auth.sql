-- Principal identity, linked to the OIDC provider's `(iss, sub)` pair.
--
-- `iss` (issuer) + `sub` (subject) is the durable composite identifier OIDC
-- guarantees stable across logins. Keying on `sub` alone would collide if the
-- operator ever points at a second IdP that happens to mint overlapping
-- subjects; (iss, sub) makes the join key unambiguous.
--
-- `email` is nullable on purpose: we only persist it when the IdP marks it
-- verified (email_verified=true on the id_token). Unverified emails would
-- otherwise let one IdP account silently bind another user's address. A
-- partial unique index enforces uniqueness over the non-NULL rows.
CREATE TABLE IF NOT EXISTS users (
    user_id    INTEGER PRIMARY KEY AUTOINCREMENT,
    iss        TEXT NOT NULL,
    sub        TEXT NOT NULL,
    email      TEXT,
    name       TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen  INTEGER,
    UNIQUE (iss, sub)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_unique
    ON users (email) WHERE email IS NOT NULL;


-- Server-side OIDC token vault. Cookie carries an opaque handle; this row
-- holds the encrypted access + refresh tokens for that grant.
--
-- Why server-side: Stackpit's web surface is a confidential OAuth2 client
-- (it owns the client_secret, it does the code exchange). The browser is
-- the user-agent, not the client. Tokens belong on the server; the cookie
-- is a claim ticket against this table -- the BFF pattern.
--
-- Why hashed handle: the `handle` column stores SHA-256 of the cookie's
-- raw handle. A read of the SQLite file yields hashes, not cookie values
-- an attacker could replay against this server. Lookups derive the hash
-- on the fly from the cookie's raw 32 bytes.
--
-- Why encrypted at rest: this table holds live IdP credentials. A read of
-- the SQLite file shouldn't yield bearer tokens that an attacker can replay
-- against Hydra. Tokens are AES-256-GCM ciphertext (see crate::crypto)
-- with the raw (not hashed) handle mixed into AAD -- blob-swapping between
-- rows fails decryption, and forging a hash preimage doesn't yield
-- plaintext without the master key. `key_id` exists for future key
-- rotation; today's encryption key is `0`.
--
-- `sid` is the id_token's session-id claim, recorded for back-channel
-- logout (Hydra sends `sid` on logout tokens; we delete matching grants).
-- Nullable because not every IdP emits it.
--
-- `csrf_token` is a per-grant synchronizer token, compared in constant time
-- against the form field. It lives here because its lifetime matches the
-- browser session.
CREATE TABLE IF NOT EXISTS oidc_grants (
    handle        BLOB PRIMARY KEY,
    user_id       INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    iss           TEXT NOT NULL,
    sub           TEXT NOT NULL,
    sid           TEXT,
    access_token  BLOB NOT NULL,
    access_exp    INTEGER NOT NULL,
    refresh_token BLOB,
    refresh_exp   INTEGER,
    -- id_token kept for RP-initiated logout (Hydra wants it as id_token_hint).
    -- Encrypted (AES-256-GCM) same way as access_token; AAD = raw handle.
    id_token      BLOB,
    key_id        INTEGER NOT NULL DEFAULT 0,
    csrf_token    TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    last_used_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Back-channel logout by sid: Hydra sends `sid`, we find the matching grant.
CREATE INDEX IF NOT EXISTS idx_oidc_grants_iss_sid
    ON oidc_grants (iss, sid) WHERE sid IS NOT NULL;

-- Back-channel logout by sub, and self-service "log out everywhere".
CREATE INDEX IF NOT EXISTS idx_oidc_grants_iss_sub
    ON oidc_grants (iss, sub);

-- Cleanup task scans by expiry.
CREATE INDEX IF NOT EXISTS idx_oidc_grants_refresh_exp
    ON oidc_grants (refresh_exp);


-- Revocation markers from back-channel logout. The bearer gate consults
-- these after a successful introspection -- they catch the window between
-- "Hydra revoked the session" and "our introspection cache TTL expires".
--
-- `kind` is 'sid' (preferred, scopes to one device) or 'sub' (whole user).
-- `expires_at` is sized to max(access_token_ttl, refresh_token_ttl); past
-- that, no live token could still claim this sid/sub, so the row is safe
-- to purge.
CREATE TABLE IF NOT EXISTS oidc_revocations (
    iss        TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('sid', 'sub')),
    value      TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (iss, kind, value)
);

CREATE INDEX IF NOT EXISTS idx_oidc_revocations_expires_at
    ON oidc_revocations (expires_at);


-- JTI replay defense for back-channel logout tokens. The spec requires we
-- reject a previously-seen jti within a reasonable window; we keep them
-- for the same TTL as the revocation marker so the dedupe window can't
-- expire while the revocation is still load-bearing.
CREATE TABLE IF NOT EXISTS oidc_logout_jti (
    jti        TEXT PRIMARY KEY,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_logout_jti_expires_at
    ON oidc_logout_jti (expires_at);
