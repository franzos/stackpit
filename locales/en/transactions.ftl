# Transactions surface: the per-project transaction list and the transaction
# detail (instances) page. Reuses nav-transactions for heading/breadcrumb/
# title. Counted strings use tv_count plurals ([one]/[other]).

# --- Page-title suffix (dynamic-prefix titles) ---
transactions-title-suffix = — Stackpit

# --- Transaction list ---
transactions-time-range = Time range
transactions-period-1h = Last hour
transactions-period-24h = Last 24h
transactions-period-7d = Last 7 days
transactions-period-14d = Last 14 days
transactions-period-30d = Last 30 days
transactions-period-90d = Last 90 days
transactions-filter-submit = Filter
transactions-list-empty = No transactions in this period.
transactions-col-name = Transaction
transactions-col-throughput = Throughput
transactions-col-failure = Failure %
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

# --- Pagination (transaction detail) ---
transactions-pagination-label = Pagination
transactions-pagination-prev = « Previous
transactions-pagination-next = Next »
transactions-detail-count = { $count ->
    [one] { $count } instance
   *[other] { $count } instances
}
