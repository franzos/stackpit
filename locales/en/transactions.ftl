# Transactions surface: the per-project transaction list and the transaction
# detail (instances) page. Reuses nav-transactions for heading/breadcrumb/
# title. Counted strings use tv_count plurals ([one]/[other]).

# --- Page-title suffix (dynamic-prefix titles) ---
transactions-title-suffix = — Stackpit

# --- Transaction list ---
transactions-time-range = Time range
transactions-filter-submit = Filter
transactions-list-empty = No transactions in this period.
transactions-col-name = Transaction
transactions-col-throughput = Throughput
transactions-col-failure = Failure %
# The summary line already renders the value with its own `%`, so the shared
# column header would read "Failure % 0.0 %" there.
transactions-detail-failure-label = Failure
transactions-col-count = Count
transactions-col-users = Users

# --- Transaction detail (instances) ---
transactions-detail-op = op:
transactions-detail-empty = No instances recorded for this transaction.
transactions-detail-col-duration = Duration
transactions-detail-col-status = Status
transactions-detail-col-trace = Trace
transactions-detail-col-when = When
transactions-detail-distribution = Duration distribution
transactions-detail-spans = Span breakdown
transactions-detail-issues = Related issues
transactions-detail-instances = Slowest instances
transactions-detail-trend = Percentile trend
transactions-detail-trend-note = Marked points are where p95 exceeded 1.5× the median of the five points before it.

# --- Pagination (transaction detail) ---
transactions-pagination-label = Pagination
transactions-pagination-prev = « Previous
transactions-pagination-next = Next »
transactions-detail-count = { $count ->
    [one] { $count } instance
   *[other] { $count } instances
}
