# Replays surface: the per-project replay list and the replay detail page.
# Reuses nav-replays. Counted strings use tv_count plurals ([one]/[other]).

# --- Page-title suffix ---
replays-title-suffix = — Stackpit

# --- Replay list ---
replays-list-empty = No replays found. Replay events will appear here.
replays-col-event-id = Event ID
replays-col-type = Type
replays-col-release = Release
replays-col-url = URL
replays-col-user = User
replays-col-browser = Browser
replays-col-duration = Duration
replays-col-errors = Errors
replays-col-environment = Environment
replays-col-timestamp = Timestamp

# --- Replay detail ---
replays-detail-heading = Replay
replays-detail-note = Recording playback not yet available. Raw replay data is shown below.
replays-detail-raw-payload = Raw payload

# --- Errors captured during the replay (from the payload's error_ids) ---
replays-related-errors = Errors in this replay
replays-col-level = Level
replays-col-title = Title

# --- Pagination ---
replays-pagination-label = Pagination
replays-pagination-prev = « Previous
replays-pagination-next = Next »
replays-count = { $count ->
    [one] { $count } replay
   *[other] { $count } replays
}
