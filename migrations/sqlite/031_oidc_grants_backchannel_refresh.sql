-- refresh_token_bc is a second copy of a non-offline grant's refresh token,
-- encrypted with the hashed handle as AAD, so back-channel logout can revoke it
-- without the cookie handle. The database plus the master key can read it.
ALTER TABLE oidc_grants ADD COLUMN offline INTEGER NOT NULL DEFAULT 0;
ALTER TABLE oidc_grants ADD COLUMN refresh_token_bc BLOB;
