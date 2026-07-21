# Issues surface: the fingerprint-grouped issue list and the issue detail page.
# issue-detail-exception-stacktrace carries an inline &amp; and is rendered with
# |safe. Counted strings use tv_count plurals ([one]/[other]).

# --- Shared labels (issue list + issue detail) ---
issues-label-title = Title
issues-label-level = Level
issues-label-events = Events
issues-label-users = Users
issues-label-trend = Trend
issues-trend-tooltip = Event volume over the selected period
issues-label-status = Status
issues-label-first-seen = First seen
issues-label-last-seen = Last seen
issues-label-value = Value

# --- Status values (filter options + badges) ---
issues-status-unresolved = Unresolved
issues-status-resolved = Resolved
issues-status-ignored = Ignored

# --- Pagination (shared) ---
issues-pagination-label = Pagination
issues-pagination-prev = « Previous
issues-pagination-next = Next »

# --- Page-title suffix (dynamic-prefix titles) ---
issues-title-suffix = — Stackpit

# --- Issue list ---
issues-list-subtitle = Issues grouped by fingerprint.
issues-list-filtered-by-tag = Filtered by tag:
issues-list-clear-tag = Clear tag filter
issues-list-search-placeholder = Search issues…
issues-list-search-label = Search issues
issues-list-select = Select issue
issues-list-filter-status = Filter by status
issues-list-status-all = All statuses
issues-list-filter-level = Filter by level
issues-list-level-all = All levels
issues-list-filter-release = Filter by release
issues-list-release-all = All releases
issues-period-label = Time range
issues-period-all = All time
issues-period-1h = Last hour
issues-period-24h = Last 24h
issues-period-7d = Last 7 days
issues-period-14d = Last 14 days
issues-period-30d = Last 30 days
issues-period-90d = Last 90 days
issues-period-365d = Last 365 days
issues-list-filter-submit = Filter
issues-list-empty = No issues match the current filters.
issues-untitled = (untitled)

# --- Bulk actions ---
issues-bulk-resolve-all = Resolve all { $count }
issues-bulk-ignore-all = Ignore all { $count }
issues-bulk-delete-all = Delete all { $count }
issues-bulk-resolve-confirm = { $count ->
    [one] Resolve all { $count } matching issue?
   *[other] Resolve all { $count } matching issues?
}
issues-bulk-ignore-confirm = { $count ->
    [one] Ignore all { $count } matching issue?
   *[other] Ignore all { $count } matching issues?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] Permanently delete all { $count } matching issue?
   *[other] Permanently delete all { $count } matching issues?
}
issues-bulk-resolve = Resolve
issues-bulk-ignore = Ignore
issues-bulk-delete = Delete
issues-bulk-delete-selected-confirm = Permanently delete selected issues?

# --- Count (pagination) ---
issues-count = { $count ->
    [one] { $count } issue
   *[other] { $count } issues
}

# --- Issue detail ---
issue-detail-title-fallback = Issue
issue-detail-resolve = ✓ Resolve
issue-detail-reopen = Re-open
issue-detail-unignore = Un-ignore
issue-detail-create-external-issue = Create issue
issue-detail-tab-details = Details
issue-detail-tab-events = All events
issue-detail-exception-stacktrace = Exception &amp; Stacktrace
issue-detail-handled = handled
issue-detail-unhandled = unhandled
issue-detail-in = in
issue-detail-var-name = Variable
issue-detail-no-source = No source context available
issue-detail-minified-hint = These frames look minified and no source map was applied.
issue-detail-minified-hint-link = Upload source maps
issue-detail-breadcrumbs = Breadcrumbs
issue-detail-th-time = Time
issue-detail-th-category = Category
issue-detail-th-message = Message
issue-detail-crumb-data = data
issue-detail-tags = Tags
issue-detail-contexts = Contexts
issue-detail-additional-data = Additional data
issue-detail-view-replay = View replay
issue-detail-view-trace = View trace
issue-detail-request = Request
issue-detail-headers = Headers
issue-detail-th-header = Header
issue-detail-query-string = Query string
issue-detail-body = Body
issue-detail-environment = Environment
issue-detail-user-reports = User reports
issue-detail-anonymous = Anonymous
issue-detail-attachments = Attachments
issue-detail-att-filename = Filename
issue-detail-att-type = Type
issue-detail-att-size = Size
issue-detail-download = Download
issue-detail-raw-json = Raw JSON
issue-detail-no-events = No events found for this issue.
issue-detail-ev-id = Event ID
issue-detail-ev-timestamp = Timestamp
issue-detail-ev-platform = Platform
issue-detail-events-count = { $count ->
    [one] { $count } event
   *[other] { $count } events
}
issue-detail-props-heading = Issue properties
issue-detail-fingerprint = Fingerprint
issue-detail-external-tracker = External tracker
issue-detail-view-on = View on
issue-detail-tag-facets = Tag facets
issue-detail-discard-undo-title = Resume accepting future events with this fingerprint
issue-detail-discard-undo = Undo discard
issue-detail-discard-confirm = Discard all future events with this fingerprint?
issue-detail-discard-title = Silently drop future events matching this fingerprint
issue-detail-discard = Discard future events
flash-tracker-create-failed = Could not create the tracker issue. Check the integration token and repository, then try again.
flash-tracker-config-incomplete = This tracker integration is missing a repository or token. Fix it in the integration settings.
