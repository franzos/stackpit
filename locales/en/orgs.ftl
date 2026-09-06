# Organizations surface: the org list (templates/orgs.html), the members/invites
# page (templates/org_members.html), and the standalone invite-accept page
# (templates/invite_accept.html, invite-* keys). Reuses nav-organizations and
# common-action-save. Separator spaces live in the template. The delete-danger
# sentence and the "Type <slug> to confirm" label are split at their {{ var }}
# interpolations. Enum values (member/owner, status) stay literal in the template.
orgs-page-title = Organizations - Stackpit
orgs-subtitle = The organizations you belong to. Switch between them or create a new one.
orgs-empty = You are not a member of any organization yet.
orgs-col-organization = Organization
orgs-col-kind = Kind
orgs-members-btn = Members
orgs-active = Active
orgs-switch = Switch
orgs-create-heading = Create organization
orgs-create-desc = You become the owner. The slug is derived from the name when left blank.
orgs-name = Name
orgs-slug = Slug
orgs-optional = (optional)
orgs-create-submit = Create organization

# --- Members page ---
orgs-members-title-suffix = Members - Stackpit
orgs-members-word = Members
orgs-organization-word = organization
orgs-slug-heading = Slug
orgs-slug-desc = Identifies this organization in URLs. Must be unique.
orgs-email = Email
orgs-role = Role
orgs-role-member = member
orgs-role-owner = owner
orgs-member-fallback = user #{ $id }
orgs-joined = Joined
orgs-promote = Promote
orgs-demote = Demote
orgs-remove = Remove
orgs-invites-heading = Invites
orgs-created = Created
orgs-expires = Expires
orgs-status = Status
orgs-revoke = Revoke
orgs-create-invite-heading = Create invite
orgs-create-invite-desc = Generates a single-use invite link.
orgs-expiry-label = Expiry (seconds)
orgs-expiry-hint = (optional, default 7 days)
orgs-create-invite-submit = Create invite
orgs-forseti-note = Membership for this organization is managed externally.
orgs-personal-note = This is a personal organization. Membership is not configurable.
orgs-danger-heading = Danger zone
orgs-delete-danger-pre = Deleting removes
orgs-delete-danger-projects = project(s),
orgs-delete-danger-members = member(s),
orgs-delete-danger-rest = and all events, issues, keys, alerts and integrations. This cannot be undone.
orgs-confirm-type-pre = Type
orgs-confirm-type-post = to confirm
orgs-delete-confirm = Delete this organization and ALL its data. This cannot be undone.
orgs-delete-submit = Delete organization

# --- Invite accept (standalone page) ---
invite-page-title = Organisation invite - Stackpit
invite-heading = Organisation invite
invite-back-projects = Back to projects
invite-intro-pre = You have been invited to join
invite-intro-as = as
invite-intro-post = .
invite-accept-btn = Accept invite
invite-decline = Decline
invite-error-accepted = This invite has already been accepted.
invite-error-expired = This invite has expired.
invite-error-email-mismatch = This invite is for a different email address. Ask for an invite without an email restriction, or sign in with the matching account.

# Validation/error messages rendered on the html_error page, localized at the
# call sites that carry a request locale. Internal 5xx failures stay English.
orgs-err-name-required = Organization name is required.
orgs-err-slug-taken = That slug is already taken.
orgs-err-invite-not-found = Invite not found or invalid.
orgs-err-org-not-found = Organization not found.
orgs-err-last-owner-remove = The last owner cannot be removed.
orgs-err-last-owner-demote = The last owner cannot be demoted.
orgs-err-confirm-slug = Type the organization slug to confirm deletion.
orgs-err-not-deletable = This organization cannot be deleted.
orgs-err-license-cap-reached = Your license's organization limit is reached. Remove an organization or upgrade the license to create another.
orgs-err-limit-reached = { $count ->
    [one] You have reached the limit of { $count } organization.
   *[other] You have reached the limit of { $count } organizations.
}
