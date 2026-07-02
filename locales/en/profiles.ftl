# Profiles surface: the per-project profile list and the profile detail page.
# Reuses nav-profiles. Counted strings use tv_count plurals ([one]/[other]).

# --- Page-title suffix ---
profiles-title-suffix = — Stackpit

# --- Profile list ---
profiles-list-empty = No profiles found. Profile events with <code class="text-mono">item_type = "profile"</code> will appear here.
profiles-col-event-id = Event ID
profiles-col-transaction = Transaction
profiles-col-platform = Platform
profiles-col-release = Release
profiles-col-environment = Environment
profiles-col-timestamp = Timestamp

# --- Profile detail ---
profiles-detail-heading = Profile
profiles-detail-raw-payload = Raw payload

# --- Pagination ---
profiles-pagination-label = Pagination
profiles-pagination-prev = « Previous
profiles-pagination-next = Next »
profiles-count = { $count ->
    [one] { $count } profile
   *[other] { $count } profiles
}
