# Releases surface: the cross-project release list and the per-project release
# health page. Reuses nav-releases and nav-health. Counted strings use
# tv_count plurals ([one]/[other]).

# --- Page-title suffix ---
releases-title-suffix = — Stackpit

# --- Release list ---
releases-list-search-placeholder = Search releases…
releases-list-search-label = Search releases
releases-list-project-placeholder = Project ID
releases-list-project-label = Filter by project
releases-list-period-label = Adoption period
releases-list-period-24h = Last 24h
releases-list-period-7d = Last 7 days
releases-list-period-30d = Last 30 days
releases-filter-submit = Filter
releases-list-empty = No releases yet. Set a <code class="text-mono">release</code> on your SDK and they'll appear here once events arrive.
releases-col-version = Version
releases-col-project = Project
releases-col-issues = Issues
releases-col-events = Events
releases-col-adoption = Adoption
releases-col-first-seen = First seen
releases-col-last-seen = Last seen

# --- Pagination ---
releases-pagination-label = Pagination
releases-pagination-prev = « Previous
releases-pagination-next = Next »
releases-count = { $count ->
    [one] { $count } release
   *[other] { $count } releases
}

# --- Release health ---
release-health-title = Release Health
release-health-heading = Release health
release-health-sessions-heading = Sessions over time
release-health-empty = No session data available. Session events with a <code class="text-mono">status</code> field will appear here.
release-health-col-release = Release
release-health-col-sessions = Sessions
release-health-col-ok = OK
release-health-col-crashed = Crashed
release-health-col-errored = Errored
release-health-col-crash-free-sessions = Crash-free sessions
release-health-col-error-free-sessions = Error-free sessions
release-health-col-crash-free-users = Crash-free users
release-health-na = n/a
release-health-subtitle = Session outcomes are SDK-reported health pings, not error events. Click a release to see its issues.
release-health-crashed-title = View issues for this release
release-health-errored-title = View issues for this release
release-health-errored-hint = Errored counts are SDK-reported session health pings (a session that logged a handled error but did not crash), not individual error events, and cannot be listed per session. The linked issues are the error groups seen in this release.

# --- Release detail (per-version) ---
release-detail-sessions-heading = Session health
release-detail-sessions-note = SDK-reported session outcomes (ok / errored / crashed). These are health pings, not individual error events.
release-detail-no-health = No session data for this release.
release-detail-issues-heading = Issues in this release
release-detail-issues-note = Distinct error groups first or last seen with this release.
release-detail-no-issues = No issues recorded for this release.
