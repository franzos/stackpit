# Projects surface: list, new, settings (general/keys/sourcemaps/filters),
# integrations, and the created confirmation. Values rendered with |safe carry
# inline HTML markup and are noted below.

# --- Project list ---
projects-list-title = Projects — Stackpit
projects-list-heading = Projects
projects-list-subtitle = Monitor health across your entire architecture.
projects-list-all-events = All events
projects-list-all-releases = All releases
projects-list-new = + New project
projects-list-search-placeholder = Query projects by name, platform, or owner…
projects-list-search-label = Search projects
projects-list-filter = Filter
projects-org-filter-label = Filter by organization
projects-org-filter-all = All organizations
projects-list-empty = No projects found. Events will appear here once ingested.
projects-period-label = Time range
projects-period-all = All time
projects-period-1h = Last hour
projects-period-24h = Last 24 hours
projects-period-7d = Last 7 days
projects-period-14d = Last 14 days
projects-period-30d = Last 30 days
projects-period-90d = Last 90 days
projects-period-365d = Last 365 days
projects-col-project = Project
projects-col-platforms = Platforms
projects-col-issues = Issues
projects-col-events = Events
projects-col-breakdown = Breakdown
projects-col-release = Release
projects-col-first-seen = First seen
projects-col-last-seen = Last seen
projects-breakdown-errors = Errors:
projects-breakdown-transactions = Transactions:
projects-breakdown-sessions = Sessions:
projects-breakdown-other = Other:
projects-legend-errors = Errors
projects-legend-transactions = Transactions
projects-legend-sessions = Sessions
projects-legend-other = Other

# --- Shared across project forms ---
projects-optional = (optional)
projects-cancel = Cancel
projects-remove = Remove
projects-delete = Delete
projects-name-placeholder = My Project

# --- New project ---
projects-new-title = New project — Stackpit
projects-new-heading = New project
projects-new-name-label = Project name
projects-new-platform-label = Platform
projects-new-platform-select = Select a platform…
projects-new-platform-other = Other
projects-new-platform-native = Native (C/C++)
projects-new-submit = Create project

# --- Settings tabs (shared by the settings pages) ---
projects-tab-general = General
projects-tab-sdk = SDK setup
projects-tab-sourcemaps = Source maps
projects-tab-filters = Filters
projects-tab-integrations = Integrations

# --- Settings: general ---
projects-settings-heading = Settings
projects-settings-archived = (archived)
projects-settings-name-heading = Project name
projects-settings-display-name = Display name
projects-settings-save-name = Save name
projects-settings-info-heading = Project info
projects-settings-status = Status
projects-settings-source = Source
projects-repos-heading = Source repositories
projects-repos-help = Link stack frames to source code on your forge. Register a release with a commit SHA via <code class="text-mono">sentry-cli</code> to activate links.
projects-repos-empty = No repositories configured.
projects-repos-url-label = Repository URL
projects-repos-col-forge = Forge
projects-repos-template = URL template
projects-repos-auto = auto
projects-repos-remove-confirm = Remove this repository?
projects-repos-add = Add repository
projects-repos-add-help = Adds clickable source links (e.g. "View on GitHub") next to stack frames. Requires a release with a commit SHA — forge type auto-detected. Supported: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. For other forges, provide a URL template.
projects-danger-heading = Danger zone
projects-archive-desc = Archive this project. Archived projects reject new events.
projects-archive-confirm = Archive this project? New events will be rejected.
projects-archive-submit = Archive project
projects-unarchive-desc = Unarchive this project to resume accepting events.
projects-unarchive-submit = Unarchive project
projects-delete-desc = Permanently delete this project and all its data. This cannot be undone.
projects-delete-confirm = Delete this project and ALL its data? This cannot be undone.
projects-delete-submit = Delete project
projects-move-heading = Move to organization
projects-move-desc = Move this project to another organization you own. Its data and DSNs stay valid, but notification integrations are unlinked and must be re-added in the new organization.
projects-move-target-label = Destination organization
projects-move-confirm-pre = Type
projects-move-confirm-post = to confirm.
projects-move-confirm-placeholder = Project name
projects-move-confirm-dialog = Move this project to the selected organization?
projects-move-submit = Move project
projects-move-err-invalid-target = Invalid destination organization.
projects-move-err-name-mismatch = The project name does not match.
projects-move-err-denied = You are not an owner of the destination organization.
projects-move-err-conflict = The project could not be moved; it may have changed. Please try again.

# --- Settings: SDK setup / keys ---
projects-keys-title = SDK Setup
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = No keys registered. Create a key below to get a DSN.
projects-keys-list-heading = Project keys
projects-keys-empty = No keys registered for this project.
projects-keys-col-public = Public key
projects-keys-col-label = Label
projects-keys-col-status = Status
projects-keys-col-created = Created
projects-keys-delete-confirm = Delete this key? SDKs using it will stop working.
projects-keys-create-heading = Create key
projects-keys-label-label = Label
projects-keys-label-placeholder = e.g. production, staging
projects-keys-create-submit = Create key

# --- Settings: source maps ---
projects-sourcemaps-title = Source Maps
projects-sourcemaps-apikey-heading = API key
projects-sourcemaps-apikey-desc = Source map uploads require an API key. Specific to this project and only usable for source map operations.
projects-sourcemaps-key-generated = Key generated:
projects-sourcemaps-key-warning = Copy this key now — it will not be shown again.
projects-sourcemaps-col-key = Key
projects-sourcemaps-regen-confirm = Regenerate key? The current key will stop working.
projects-sourcemaps-regen = Regenerate
projects-sourcemaps-empty = No source map API key for this project.
projects-sourcemaps-generate = Generate key
projects-sourcemaps-setup-heading = Setup
projects-sourcemaps-setup-desc = Use <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> to upload source maps. Set these environment variables:
projects-sourcemaps-then-upload = Then upload:

# --- Settings: filters ---
projects-filters-inbound-heading = Inbound filters
projects-filters-inbound-desc = Built-in filters that drop events matching common noise patterns.
projects-filters-browser-ext = Browser extensions — drop events from Chrome/Firefox/Safari extensions
projects-filters-localhost = Localhost — drop events from localhost, 127.0.0.1, private IPs
projects-filters-inbound-submit = Save inbound filters
projects-filters-message-heading = Message filters
projects-filters-message-help = Glob patterns matched against event titles. Use <code class="text-mono">*</code> for any sequence, <code class="text-mono">?</code> for single char.
projects-filters-col-pattern = Pattern
projects-filters-message-empty = No message filters configured.
projects-filters-add-pattern = Add pattern
projects-filters-message-submit = Add message filter
projects-filters-ratelimit-heading = Rate limit
projects-filters-ratelimit-desc = Maximum events per minute for this project. 0 = unlimited.
projects-filters-ratelimit-label = Events per minute
projects-filters-ratelimit-submit = Save rate limit
projects-filters-env-heading = Excluded environments
projects-filters-env-desc = Events from these environments will be silently dropped.
projects-filters-col-environment = Environment
projects-filters-env-empty = No excluded environments.
projects-filters-env-add-label = Add excluded environment
projects-filters-env-submit = Exclude environment
projects-filters-release-heading = Release filters
projects-filters-release-desc = Glob patterns matched against release versions. Matching events are dropped.
projects-filters-release-empty = No release filters.
projects-filters-release-submit = Add release filter
projects-filters-ua-heading = User-agent filters
projects-filters-ua-desc = Glob patterns matched against User-Agent headers. Built-in patterns for kube-probe and health checkers are always active.
projects-filters-ua-empty = No custom user-agent filters.
projects-filters-ua-submit = Add user-agent filter
projects-filters-rules-heading = Custom rules
projects-filters-rules-desc = Advanced rules that match event fields. Higher priority rules are evaluated first.
projects-filters-col-field = Field
projects-filters-col-operator = Operator
projects-filters-col-value = Value
projects-filters-col-action = Action
projects-filters-col-priority = Priority
projects-filters-rules-empty = No custom rules.
projects-filters-sample-rate-label = Sample rate
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Add rule
projects-filters-op = { $op ->
    [not_equals] not equals
    [contains] contains
    [not_contains] does not contain
    [starts_with] starts with
    [in] in
    [not_in] not in
   *[equals] equals
}
projects-filters-action = { $action ->
    [sample] sample
   *[drop] drop
}
projects-filters-ip-heading = IP blocklist
projects-filters-ip-desc = CIDR blocks or individual IPs. Events from blocked IPs are silently dropped.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = No IP blocks configured.
projects-filters-ip-add-label = Add CIDR
projects-filters-ip-submit = Block IP range
projects-filters-discard-heading = Discard stats
projects-filters-discard-window = (last 7 days)
projects-filters-col-date = Date
projects-filters-col-reason = Reason
projects-filters-col-count = Count

# Filter entity labels, interpolated into flash-not-found-filter on delete.
projects-filter-label-message = message filter
projects-filter-label-environment = environment filter
projects-filter-label-release = release filter
projects-filter-label-user-agent = user-agent filter
projects-filter-label-rule = filter rule

# --- Settings: integrations ---
projects-integrations-active-heading = Active integrations
projects-integrations-active-empty = No integrations activated. Add a global integration on the <a class="text-primary" href="/web/settings/integrations/">Integrations</a> page first, then enable it here. You can scope each one by minimum level and environment so dev-noise stays out of prod channels.
projects-integrations-deactivate-confirm = Deactivate this integration for the project?
projects-integrations-deactivate = Deactivate
projects-integrations-notify-new-issues = New issues
projects-integrations-notify-regressions = Regressions
projects-integrations-notify-threshold = Threshold alerts
projects-integrations-notify-digests = Digests
projects-integrations-min-level = Minimum level
projects-integrations-level-any = Any
projects-integrations-env-filter = Environment filter
projects-integrations-env-placeholder = e.g. production
projects-integrations-to-address = To address
projects-integrations-to-address-note = (email integrations only)
projects-integrations-activate-heading = Activate integration
projects-integrations-integration-label = Integration
projects-integrations-activate-submit = Activate
projects-integrations-available-empty = No integrations available. <a class="text-primary" href="/web/settings/integrations/">Create one first</a>.
projects-integrations-tracker-hint = Overrides the org-level tracker for this project. Leave blank to fall back to the default.
projects-integrations-tracker-owner = Owner
projects-integrations-tracker-repo = Repository
projects-integrations-tracker-project-id = Project ID
projects-integrations-tracker-project-id-note = (GitLab only)
projects-integrations-tracker-save = Save target

# --- Project created ---
projects-created-word = created
projects-created-breadcrumb = Created
projects-created-heading = Project created
projects-created-subtitle = Use the DSN below to configure your SDK.
projects-created-settings-btn = Project settings
projects-created-back = Back to projects
projects-created-details-heading = Project details
projects-created-col-id = Project ID
projects-created-sdk-desc-before = Install the Sentry SDK for
projects-created-sdk-desc-after = and initialize it with the DSN above.
projects-created-docs-javascript = Sentry JavaScript docs →
projects-created-docs-python = Sentry Python docs →
projects-created-docs-rust = Sentry Rust docs →
projects-created-docs-go = Sentry Go docs →
projects-created-docs-node = Sentry Node.js docs →
projects-created-docs-java = Sentry Java docs →
projects-created-docs-ruby = Sentry Ruby docs →
projects-created-docs-php = Sentry PHP docs →
projects-created-docs-elixir = Sentry Elixir docs →
projects-created-docs-dotnet = Sentry .NET docs →
projects-created-docs-apple = Sentry Apple docs →
projects-created-docs-kotlin = Sentry Kotlin docs →
projects-created-docs-native = Sentry Native docs →
projects-created-docs-generic = Sentry platform docs →
