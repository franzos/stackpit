# Integrations settings surface: the list (templates/integrations.html) and the
# three "add" forms (webhook, slack, email). Reuses nav-settings/nav-integrations
# for chrome. Separator spaces live in the template. integrations-empty carries
# inline <strong> markup and the arrow glyph, rendered with |safe.
integrations-page-title = Integrations — Stackpit
integrations-subtitle = Webhook, Slack and email outputs. Per-project routing is set under each project's settings.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + Email
integrations-license-required-badge = Needs license
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
integrations-tracker-token-label = API token
integrations-tracker-token-placeholder = Personal access token
integrations-tracker-target-help = Which repository this files into comes from each project's own repository settings, so it is not configured here. Add the repository under the project's settings.
integrations-global-label = Deliver to every project
integrations-global-help = Alerts go to every project in this organization, except the ones you exclude on this integration's page. Per-project level and environment filters still apply on top.
integrations-global-badge = org-wide
integrations-global-save = Save routing
integrations-global-on = Deliver org-wide
integrations-global-off = Stop delivering org-wide

# Integration detail: per-project routing
integrations-detail-title = Integration — Stackpit
integrations-back = Back to integrations
integrations-projects-heading = Project routing
integrations-projects-hint-global = This integration delivers to every project below unless you exclude it. Excluding is the only opt-out; there is no include list.
integrations-projects-hint-per-project = This integration only delivers where a project has activated it. Mark it org-wide to deliver everywhere instead.
integrations-projects-hint-tracker = Issue trackers match a project's repositories by forge and host. Excluding a project keeps this tracker out of its filing options.
integrations-projects-empty = This organization has no projects yet.
# Header summary, counted across the org rather than the current page.
integrations-summary-delivering = { $count ->
    [one] { $count } delivering
   *[other] { $count } delivering
}
integrations-summary-excluded = { $count ->
    [one] { $count } excluded
   *[other] { $count } excluded
}
integrations-summary-inert = { $count ->
    [one] { $count } not routed
   *[other] { $count } not routed
}
integrations-search-placeholder = Filter by project name
integrations-search-label = Filter projects
integrations-search-submit = Filter
integrations-sort-label = Sort projects
integrations-sort-state = Delivering first
integrations-sort-name = By name
integrations-pagination-label = Project routing pages
integrations-projects-count = { $count ->
    [one] { $count } project
   *[other] { $count } projects
}
integrations-col-project = Project
integrations-col-state = State
integrations-project-archived = archived
integrations-state-default = Delivering
integrations-state-customised = Customised
integrations-state-excluded = Excluded
integrations-state-no-repo = No matching repository
integrations-state-not-routed = Not activated
integrations-exclude = Exclude
integrations-include = Include
integrations-email-to-label = Default recipient
integrations-email-to-help = Used where a project has not set its own To address. Required for an org-wide integration.
