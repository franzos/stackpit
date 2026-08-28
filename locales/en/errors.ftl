# Standalone error page (src/html/mod.rs html_error) and the invite-created
# confirmation page (src/html/orgs.rs). Both render without request context, so
# their strings resolve at the default locale (English) and cannot follow the
# request locale until a signature change lands. The brand word "Stackpit" stays
# literal in the templates, matching base.html/login.html.
error-page-title = Error - Stackpit
error-heading = Error
error-not-found = The page you requested does not exist.
error-back-projects = Back to projects

# Invite-created confirmation page (English-only, no request context).
invite-created-page-title = Invite created - Stackpit
invite-created-heading = Invite created
invite-created-share = Share this link. It is valid for { $ttl } and single-use.
invite-created-back-members = Back to members

# --- Flash / success / validation messages (locale-aware) ---
# Emitted by the web handlers as one-shot banner text. Grouped here rather than
# per-surface because they share the flash lifecycle. The dynamic "Error: {e}"
# prefix is prepended in Rust via common-error-prefix.

# Not-found diagnostics. The "Error:"/"Fehler:" prefix is prepended in Rust; the
# value carries only the entity phrase plus the id.
flash-not-found-project = project not found: { $id }
flash-not-found-key = API key not found: { $id }
flash-not-found-integration = integration not found: { $id }
flash-not-found-alert-rule = alert rule not found: { $id }
flash-not-found-digest-schedule = digest schedule not found: { $id }
flash-not-found-repo = repository not found: { $id }
flash-not-found-project-integration = project integration not found: { $id }
flash-not-found-filter = { $label } not found

# Filter-rule validation
flash-unrecognized-field = Unrecognized field: { $value }
flash-unrecognized-operator = Unrecognized operator: { $value }
flash-unrecognized-action = Unrecognized action: { $value }

# Project settings
flash-project-name-updated = Project name updated
flash-project-name-too-long = Project name exceeds max length of { $max } characters
flash-repo-url-required = Repository URL is required
flash-repo-url-too-long = Repository URL exceeds max length of 2048 characters
flash-repo-added = Repository added
flash-repo-removed = Repository removed
flash-project-archived = Project archived
flash-project-unarchived = Project unarchived
flash-key-created = Key created
flash-key-deleted = Key deleted

# Alerts and digests
flash-project-not-found-or-denied = Error: project not found or access denied
flash-alert-rule-created = Alert rule created
flash-alert-rule-deleted = Alert rule deleted
flash-digest-schedule-created = Digest schedule created
flash-digest-schedule-deleted = Digest schedule deleted

# Project integrations
flash-integration-not-found = Integration not found
flash-integration-activated = Integration activated
flash-integration-updated = Integration updated
flash-integration-deactivated = Integration deactivated

# Org integrations
flash-name-required = Name is required
flash-invalid-integration-kind = Invalid integration kind
flash-invalid-email-provider = Invalid email provider
flash-api-token-required = API token is required.
flash-from-address-required = From address is required.
flash-email-not-configured = Email is not configured. Add an [email] section with a provider to the server config.
flash-smtp-not-configured = SMTP is not configured. Set [email] provider = "smtp" (with host) in the server config.
flash-invalid-to-address = Recipient must be a valid email address.
flash-test-digest-sent = Test digest queued for { $count } project(s) to their digest-enabled integrations.
flash-test-digest-sample = No recent activity, so a labeled sample digest was queued.
flash-test-digest-no-target = No integration has digests enabled for this schedule's project.
flash-url-required = URL is required
flash-secret-not-configured = Cannot store secret: encryption is not configured. Set STACKPIT_MASTER_KEY to enable secret storage.
flash-integration-license-required = Slack, webhook and issue-tracker integrations need an active commercial license. Email notifications stay available without one.
flash-integration-created = Integration created
flash-integration-name-exists = An integration with that name already exists.
flash-integration-deleted = Integration deleted
flash-integration-no-url = Integration has no URL configured
flash-test-notification-sent = Test notification sent

# Inbound filters
flash-inbound-filters-updated = Inbound filters updated
flash-pattern-required = Pattern is required
flash-message-filter-added = Message filter added
flash-message-filter-removed = Message filter removed
flash-rate-limit-updated = Rate limit updated
flash-environment-required = Environment is required
flash-environment-excluded = Environment excluded
flash-environment-filter-removed = Environment filter removed
flash-release-filter-added = Release filter added
flash-release-filter-removed = Release filter removed
flash-ua-filter-added = User-agent filter added
flash-ua-filter-removed = User-agent filter removed
flash-rule-added = Rule added
flash-rule-removed = Rule removed
flash-cidr-required = CIDR is required
flash-invalid-cidr = Invalid CIDR format
flash-ip-block-added = IP block added
flash-ip-block-removed = IP block removed

# New project
flash-project-name-required = Project name is required
flash-integration-saved = Integration updated
flash-integration-global-not-for-trackers = Issue trackers do not use org-wide routing; which repository they file into comes from each project's repository settings.
flash-project-excluded = Project excluded from this integration
flash-project-included = Project no longer excluded
flash-global-email-needs-recipient = An org-wide email integration needs a default recipient; projects that never activated it have no address of their own.
flash-queue-item-not-found = Queued notification not found
flash-queue-replayed = Notification delivered and removed from the queue
flash-queue-replay-failed = Replay failed: { $error }
# Replay redirects, so the banner can only carry a key. The provider's own
# message goes to the row's last_error, beside the item it belongs to.
flash-queue-replay-failed-generic = Replay failed. The reason is on the queued item, under Error.
flash-queue-cancelled = Queued notification discarded

# Licence activation
flash-license-activated = License activated
flash-license-deactivated = License removed
flash-license-persist-failed = The license verified but could not be saved. Check the server log.
flash-license-clear-failed = The license could not be removed. Check the server log.
flash-license-empty = Paste your license key to activate.
flash-license-bad-signature = This license isn't valid for this installation. Double-check you pasted the right key.
flash-license-wrong-product = This license isn't for Stackpit.
flash-license-unreadable = We couldn't read that license. Please check it and try again.
