# Events surface: the cross-project event list and the event detail page.
# event-detail-exception-stacktrace carries an inline &amp; and is rendered with
# |safe. Counted strings use tv_count plurals ([one]/[other]).

# --- Shared labels (event list + event detail) ---
events-label-title = Title
events-label-type = Type
events-label-level = Level
events-label-platform = Platform
events-label-environment = Environment
events-label-time = Time
events-label-value = Value

# --- Pagination (shared) ---
events-pagination-label = Pagination
events-pagination-prev = « Previous
events-pagination-next = Next »

# --- Page-title suffix (dynamic-prefix titles) ---
events-title-suffix = — Stackpit

# --- Event list ---
events-list-title = Events — Stackpit
events-heading = Events
events-list-search-placeholder = Search events…
events-list-search-label = Search events
events-list-select = Select event
events-list-filter-level = Filter by level
events-list-level-all = All levels
events-list-filter-type = Filter by type
events-list-type-all = All types
events-list-project-placeholder = Project ID
events-list-filter-project = Filter by project
events-list-filter-submit = Filter
events-list-empty = No events match the current filters.
events-untitled = (untitled)
events-col-project = Project

# --- Bulk actions ---
events-bulk-delete = Delete
events-bulk-delete-selected-confirm = Delete selected events?
events-bulk-delete-all = Delete all { $count } matching
events-bulk-delete-all-confirm = { $count ->
    [one] Permanently delete all { $count } matching event?
   *[other] Permanently delete all { $count } matching events?
}

# --- Count (pagination) ---
events-count = { $count ->
    [one] { $count } event
   *[other] { $count } events
}

# --- Event detail ---
event-detail-event = Event
event-detail-event-id-label = event_id:
event-detail-nav-label = Event navigation
event-detail-nav-newer = « Newer
event-detail-nav-older = Older »
event-detail-nav-count = { $count ->
    [one] { $count } event
   *[other] { $count } events
}
event-detail-nav-in-issue = in issue
event-detail-user-feedback = User feedback
event-detail-anonymous = Anonymous
event-detail-related-event = Related event:
event-detail-exception-stacktrace = Exception &amp; Stacktrace
event-detail-handled = handled
event-detail-unhandled = unhandled
event-detail-in = in
event-detail-var-name = Variable
event-detail-no-source = No source context available
event-detail-breadcrumbs = Breadcrumbs
event-detail-th-category = Category
event-detail-th-message = Message
event-detail-tags = Tags
event-detail-contexts = Contexts
event-detail-request = Request
event-detail-headers = Headers
event-detail-th-header = Header
event-detail-query-string = Query string
event-detail-body = Body
event-detail-user-reports = User reports
event-detail-attachments = Attachments
event-detail-att-filename = Filename
event-detail-att-size = Size
event-detail-download = Download
event-detail-web-vitals = Web Vitals
event-detail-raw-json = Raw JSON
event-detail-props-heading = Event properties
event-detail-prop-event-id = Event ID
event-detail-prop-timestamp = Timestamp
event-detail-prop-transaction = Transaction
event-detail-prop-release = Release
event-detail-prop-server = Server
event-detail-prop-sdk = SDK
event-detail-prop-received = Received
event-detail-user-heading = User
event-detail-user-id = ID
event-detail-user-email = Email
event-detail-user-username = Username
event-detail-user-ip = IP address

# --- Client reports (dropped-event outcomes) ---
# Reuses events-untitled and events-pagination-* (shared, same file).
client-reports-title = Client Reports
client-reports-dropped-heading = Dropped events
client-reports-dropped-subtitle = What the SDKs discarded before sending, by category and reason.
client-reports-th-category = Category
client-reports-th-reason = Reason
client-reports-th-dropped = Dropped
client-reports-empty = No client reports found for this project.
client-reports-reports-heading = Reports
client-reports-delete = Delete
client-reports-delete-selected-confirm = Delete selected reports?
client-reports-th-event-id = Event ID
client-reports-th-title = Title
client-reports-th-reasons = Reasons
client-reports-th-timestamp = Timestamp
client-reports-th-platform = Platform
client-reports-th-release = Release
client-reports-select = Select report
client-reports-delete-all = Delete all { $count }
client-reports-delete-all-confirm = { $count ->
    [one] Delete all { $count } matching report?
   *[other] Delete all { $count } matching reports?
}
client-reports-count = { $count ->
    [one] { $count } report
   *[other] { $count } reports
}

# --- User reports (user feedback) ---
# Reuses events-untitled and events-pagination-* (shared, same file).
user-reports-title = User Reports
user-reports-heading = User reports
user-reports-empty = No user reports found for this project.
user-reports-delete = Delete
user-reports-delete-selected-confirm = Delete selected reports?
user-reports-th-event-id = Event ID
user-reports-th-title = Title
user-reports-th-timestamp = Timestamp
user-reports-th-platform = Platform
user-reports-th-release = Release
user-reports-select = Select report
user-reports-delete-all = Delete all { $count }
user-reports-delete-all-confirm = { $count ->
    [one] Delete all { $count } matching report?
   *[other] Delete all { $count } matching reports?
}
user-reports-count = { $count ->
    [one] { $count } report
   *[other] { $count } reports
}
