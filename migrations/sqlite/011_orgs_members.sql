-- Cross-column CHECK invariants (personal-has-owner, single-kind, ext-link-consistent)
-- cannot be added via ALTER TABLE in SQLite. They are enforced in the query layer (Task 1.4).

ALTER TABLE organizations ADD COLUMN created_by  INTEGER REFERENCES users(user_id);
ALTER TABLE organizations ADD COLUMN is_personal INTEGER NOT NULL DEFAULT 0;
ALTER TABLE organizations ADD COLUMN ext_iss     TEXT;
ALTER TABLE organizations ADD COLUMN ext_org_id  TEXT;
ALTER TABLE organizations ADD COLUMN role_sync   INTEGER NOT NULL DEFAULT 1;

UPDATE organizations SET slug = '__system__', name = 'Unassigned' WHERE org_id = 1;

CREATE UNIQUE INDEX idx_org_ext ON organizations(ext_iss, ext_org_id)
  WHERE ext_org_id IS NOT NULL;
CREATE UNIQUE INDEX idx_org_personal_owner ON organizations(created_by)
  WHERE is_personal = 1;

CREATE TABLE organization_members (
    user_id   INTEGER NOT NULL REFERENCES users(user_id),
    org_id    INTEGER NOT NULL REFERENCES organizations(org_id),
    role      TEXT NOT NULL CHECK (role IN ('owner','member')),
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, org_id)
);

CREATE TABLE invites (
    invite_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    org_id      INTEGER NOT NULL REFERENCES organizations(org_id),
    role        TEXT NOT NULL CHECK (role IN ('owner','member')),
    token_hash  TEXT NOT NULL UNIQUE,
    email       TEXT,
    created_by  INTEGER REFERENCES users(user_id),
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    accepted_by INTEGER REFERENCES users(user_id),
    accepted_at INTEGER
);
