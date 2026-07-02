# Logs surface: the per-project log list. Reuses nav-logs. Counted strings
# use tv_count plurals ([one]/[other]).

# --- Page-title suffix ---
logs-title-suffix = — Stackpit

# --- Log list ---
logs-list-search-placeholder = Search logs…
logs-list-search-label = Search logs
logs-list-filter-level = Filter by level
logs-list-level-all = All levels
logs-filter-submit = Filter
logs-list-empty = No logs match the current filters.
logs-col-timestamp = Timestamp
logs-col-level = Level
logs-col-body = Body
logs-col-trace = Trace
logs-col-release = Release
logs-body-empty = (empty)

# --- Pagination ---
logs-pagination-label = Pagination
logs-pagination-prev = « Previous
logs-pagination-next = Next »
logs-count = { $count ->
    [one] { $count } log
   *[other] { $count } logs
}
