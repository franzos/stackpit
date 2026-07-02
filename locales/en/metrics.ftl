# Metrics surface: the per-project metric list and the metric-series detail
# page. Reuses nav-metrics. Counted strings use tv_count plurals.

# --- Page-title suffix ---
metrics-title-suffix = — Stackpit

# --- Metric list ---
metrics-list-empty = No metrics found. Metric events will appear here once received.
metrics-col-mri = MRI
metrics-col-type = Type
metrics-col-data-points = Data points
metrics-col-first-seen = First seen
metrics-col-last-seen = Last seen

# --- Pagination ---
metrics-pagination-label = Pagination
metrics-pagination-prev = « Previous
metrics-pagination-next = Next »
metrics-count = { $count ->
    [one] { $count } metric
   *[other] { $count } metrics
}

# --- Metric detail (hourly buckets) ---
metrics-detail-empty = No data points in the selected time range.
metrics-detail-col-time = Time (hourly bucket)
metrics-detail-col-count = Count
metrics-detail-col-sum = Sum
metrics-detail-col-min = Min
metrics-detail-col-max = Max
metrics-detail-col-avg = Avg
