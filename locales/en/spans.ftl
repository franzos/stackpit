# Spans surface: the per-project spans/traces list (spans-*) and the trace
# waterfall detail page (trace-detail-*). Reuses nav-spans. Counted strings
# use tv_count plurals ([one]/[other]).

# --- Page-title suffix ---
spans-title-suffix = — Stackpit

# --- Span/trace list ---
spans-list-empty = No spans found for this project.
spans-traces-heading = Traces
spans-all-heading = All spans

# --- Traces table ---
spans-col-trace-id = Trace ID
spans-col-root-op = Root op
spans-col-root-description = Root description
spans-col-duration = Duration
spans-col-first-seen = First seen
spans-col-last-seen = Last seen

# --- Aggregated spans table (grouped by op/description) ---
spans-agg-heading = Span operations
spans-col-count = Count
spans-col-p50 = p50
spans-col-p95 = p95
spans-col-avg = Avg
spans-agg-truncated = Showing the top { $count } span operations.

# --- All-spans table ---
spans-col-span-id = Span ID
spans-col-op = Op
spans-col-description = Description
spans-col-timestamp = Timestamp

# --- Pagination (span list) ---
spans-pagination-label = Pagination
spans-traces-pagination-label = Traces pagination
spans-traces-count = { $count ->
    [one] { $count } trace
   *[other] { $count } traces
}
spans-pagination-prev = « Previous
spans-pagination-next = Next »
spans-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}

# --- Org-wide traces list ---
# Reuses spans-traces-heading, spans-col-trace-id, spans-col-duration,
# spans-col-last-seen, nav-spans and the span pagination keys.
traces-col-root = Root transaction
traces-col-projects = Projects
traces-col-transactions = Transactions
traces-col-errors = Errors
traces-list-empty = No traces found.
traces-filter-projects-label = Project IDs
traces-filter-projects-placeholder = Project IDs
traces-filter-period-label = Time period
traces-filter-multi-label = Spans more than one project
traces-filter-submit = Filter
traces-pagination-label = Traces pagination
traces-count = { $count ->
    [one] { $count } trace
   *[other] { $count } traces
}
traces-more-projects = { $count ->
    [one] +{ $count } more
   *[other] +{ $count } more
}

# --- Trace detail (waterfall) ---
# title-prefix/suffix wrap the dynamic trace id; total/showing-first/of are
# split at the { $var } boundaries of the meta line.
trace-detail-title-prefix = Trace
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = total
trace-detail-showing-first = showing first
trace-detail-of = of
trace-detail-empty = No spans found for this trace.
trace-detail-col-span = Span
trace-detail-col-duration = Duration
trace-detail-root-fallback = (trace root)
trace-detail-error-title = error
trace-detail-span-fallback = span
trace-detail-compressed-note = idle gaps compressed
trace-detail-gap-title = Collapsed idle gap (no active spans)
trace-detail-lbl-span-id = Span ID
trace-detail-lbl-parent = Parent span
trace-detail-lbl-status = Status
trace-detail-lbl-start = Start offset
trace-detail-view-events = All events on this trace
trace-detail-correlated-errors = Correlated errors
trace-detail-col-level = Level
trace-detail-col-title = Title
trace-detail-col-timestamp = Timestamp
trace-detail-span-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}
trace-detail-legend-heading = Projects
trace-detail-filter-active = filtered
trace-detail-filter-clear = Show all
trace-detail-col-project = Project
trace-detail-parent-not-in-view = Parent span not in view
trace-detail-txn-badge = Transaction
