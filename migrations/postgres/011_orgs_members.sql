ALTER TABLE organizations ADD COLUMN created_by  BIGINT REFERENCES users(user_id);
ALTER TABLE organizations ADD COLUMN is_personal BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE organizations ADD COLUMN ext_iss     TEXT;
ALTER TABLE organizations ADD COLUMN ext_org_id  TEXT;
ALTER TABLE organizations ADD COLUMN role_sync   BOOLEAN NOT NULL DEFAULT TRUE;

UPDATE organizations SET slug = '__system__', name = 'Unassigned' WHERE org_id = 1;

-- org_id=1 was seeded with an explicit id and did not advance the BIGSERIAL
-- sequence; without this the first sequence-driven insert collides on the PK.
SELECT setval('organizations_org_id_seq', (SELECT MAX(org_id) FROM organizations));

ALTER TABLE organizations
  ADD CONSTRAINT org_personal_has_owner   CHECK (NOT (is_personal AND created_by IS NULL)),
  ADD CONSTRAINT org_single_kind          CHECK (NOT (is_personal AND ext_org_id IS NOT NULL)),
  ADD CONSTRAINT org_ext_link_consistent  CHECK ((ext_iss IS NULL) = (ext_org_id IS NULL));

CREATE UNIQUE INDEX idx_org_ext ON organizations(ext_iss, ext_org_id)
  WHERE ext_org_id IS NOT NULL;
CREATE UNIQUE INDEX idx_org_personal_owner ON organizations(created_by)
  WHERE is_personal;

CREATE TABLE organization_members (
    user_id   BIGINT NOT NULL REFERENCES users(user_id),
    org_id    BIGINT NOT NULL REFERENCES organizations(org_id),
    role      TEXT NOT NULL CHECK (role IN ('owner','member')),
    joined_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, org_id)
);

CREATE TABLE invites (
    invite_id   BIGSERIAL PRIMARY KEY,
    org_id      BIGINT NOT NULL REFERENCES organizations(org_id),
    role        TEXT NOT NULL CHECK (role IN ('owner','member')),
    token_hash  TEXT NOT NULL UNIQUE,
    email       TEXT,
    created_by  BIGINT REFERENCES users(user_id),
    created_at  BIGINT NOT NULL,
    expires_at  BIGINT NOT NULL,
    accepted_by BIGINT REFERENCES users(user_id),
    accepted_at BIGINT
);
