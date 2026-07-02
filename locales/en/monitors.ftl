# Monitors surface: the per-project monitor (cron check-in) list and the
# monitor detail page. Reuses nav-monitors. Counted strings use tv_count
# plurals ([one]/[other]).

# --- Page-title suffix ---
monitors-title-suffix = — Stackpit

# --- Monitor list ---
monitors-list-empty = No monitors found. Check-in events with a <code class="text-mono">monitor_slug</code> will appear here.
monitors-col-slug = Slug
monitors-col-last-status = Last status
monitors-col-last-checkin = Last check-in
monitors-col-count = Count

# --- Monitor detail ---
monitors-detail-title-prefix = Monitor
monitors-detail-subtitle = Monitor check-ins.
monitors-detail-empty = No check-ins found for this monitor.
monitors-detail-select-checkin = Select check-in
monitors-detail-confirm-delete-selected = Delete selected check-ins?
monitors-detail-delete = Delete
monitors-detail-col-title = Title
monitors-detail-col-level = Level
monitors-detail-col-environment = Environment
monitors-detail-col-time = Time
monitors-detail-untitled = (untitled)
monitors-detail-confirm-delete-all = { $count ->
    [one] Delete all { $count } check-ins?
   *[other] Delete all { $count } check-ins?
}
monitors-detail-delete-all = { $count ->
    [one] Delete all { $count }
   *[other] Delete all { $count }
}

# --- Pagination ---
monitors-pagination-label = Pagination
monitors-pagination-prev = « Previous
monitors-pagination-next = Next »
monitors-detail-count = { $count ->
    [one] { $count } check-in
   *[other] { $count } check-ins
}
