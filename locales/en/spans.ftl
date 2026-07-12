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

# --- All-spans table ---
spans-col-span-id = Span ID
spans-col-op = Op
spans-col-description = Description
spans-col-timestamp = Timestamp

# --- Pagination (span list) ---
spans-pagination-label = Pagination
spans-pagination-prev = « Previous
spans-pagination-next = Next »
spans-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
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
trace-detail-correlated-errors = Correlated errors
trace-detail-col-level = Level
trace-detail-col-title = Title
trace-detail-col-timestamp = Timestamp
trace-detail-span-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}
