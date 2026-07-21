# Integrations settings surface: the list (templates/integrations.html) and the
# three "add" forms (webhook, slack, email). Reuses nav-settings/nav-integrations
# for chrome. Separator spaces live in the template. integrations-empty carries
# inline <strong> markup and the arrow glyph, rendered with |safe.
integrations-page-title = Integrations — Stackpit
integrations-subtitle = Webhook, Slack and email outputs. Per-project routing is set under each project's settings.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + Email
integrations-add-tracker = + Issue tracker
integrations-empty = No integrations yet. Add one above to start receiving notifications. After adding, enable it per-project under <strong>Project settings → Integrations</strong>.
integrations-col-name = Name
integrations-col-type = Type
integrations-col-endpoint = Endpoint
integrations-col-created = Created
integrations-delete-confirm = Delete this integration? It will be removed from all projects.
integrations-test = Test
integrations-delete = Delete
flash-test-failed = Test failed: { $error }

# Shared form labels/buttons across the three add-integration forms.
integrations-cancel = Cancel
integrations-optional = (optional)
integrations-required = (required)
integrations-create = Create integration

# --- Add webhook ---
integrations-webhook-title = Add webhook — Stackpit
integrations-webhook-breadcrumb = Add webhook
integrations-webhook-heading = Add webhook integration
integrations-webhook-name-placeholder = e.g. Production alerts
integrations-webhook-url-label = Webhook URL
integrations-webhook-secret-label = HMAC secret
integrations-webhook-secret-placeholder = Optional signing secret

# --- Add Slack ---
integrations-slack-title = Add Slack — Stackpit
integrations-slack-breadcrumb = Add Slack
integrations-slack-heading = Add Slack integration
integrations-slack-name-placeholder = e.g. #alerts channel
integrations-slack-url-label = Slack webhook URL

# --- Add email ---
integrations-email-title = Add email — Stackpit
integrations-email-breadcrumb = Add email
integrations-email-heading = Add email integration
integrations-email-name-placeholder = e.g. Team email alerts
integrations-email-lock-pre = Provider and sender come from the server's
integrations-email-lock-post = config; this integration only picks the recipient.
integrations-email-provider-label = Provider
integrations-email-token-label = API token
integrations-email-token-placeholder-default = Leave blank to use the default
integrations-email-token-placeholder = Provider API token
integrations-email-from-label = From address
integrations-email-fromname-label = From name
integrations-email-smtp-hint = SMTP uses the server's [email] connection; no per-integration token is needed.

# --- Add issue tracker (GitHub/Forgejo/GitLab) ---
integrations-tracker-title = Add issue tracker — Stackpit
integrations-tracker-breadcrumb = Add issue tracker
integrations-tracker-heading = Add issue tracker integration
integrations-tracker-kind-label = Tracker
integrations-tracker-name-placeholder = e.g. GitHub issues
integrations-tracker-url-label = Base URL
integrations-tracker-owner-label = Owner
integrations-tracker-repo-label = Repository
integrations-tracker-project-id-label = Project ID (GitLab only)
integrations-tracker-token-label = API token
integrations-tracker-token-placeholder = Personal access token
