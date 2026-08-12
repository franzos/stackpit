# Seed keys to exercise the pipeline end to end. Full extraction is P1a.
common-action-save = Save
common-error-prefix = Error:
nav-logout = Logout
common-id-prefix = id:
common-time-just-now = just now
common-time-min-ago = { $n }m ago
common-time-hour-ago = { $n }h ago
common-time-week-ago = { $n }w ago
common-time-month-ago = { $n }mo ago
common-time-year-ago = { $n }y ago
common-time-day-ago = { $n }d ago
# Canonical time-range option set, shared by every list page (`m::period_options`).
common-period-all = All time
common-period-1h = Last hour
common-period-24h = Last 24 hours
common-period-7d = Last 7 days
common-period-14d = Last 14 days
common-period-30d = Last 30 days
common-period-90d = Last 90 days
common-period-365d = Last 365 days

# Affirmation gate above the all-matching bulk actions.
common-select-all-matching = { $count ->
    [one] Select the { $count } row matching this filter
   *[other] Select all { $count } rows matching this filter
}

test-count = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
