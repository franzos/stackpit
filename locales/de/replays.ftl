# Replay-Oberfläche: die projektbezogene Replay-Liste und die Replay-
# Detailseite. Nutzt nav-replays wieder. Zählstrings nutzen tv_count-Plurale
# ([one]/[other]).

# --- Titel-Suffix ---
replays-title-suffix = — Stackpit

# --- Replay-Liste ---
replays-list-empty = Keine Replays gefunden. Replay-Ereignisse erscheinen hier.
replays-col-event-id = Ereignis-ID
replays-col-type = Typ
replays-col-release = Release
replays-col-url = URL
replays-col-user = Benutzer
replays-col-browser = Browser
replays-col-duration = Dauer
replays-col-errors = Fehler
replays-col-environment = Umgebung
replays-col-timestamp = Zeitstempel

# --- Replay-Detail ---
replays-detail-heading = Replay
replays-detail-note = Wiedergabe der Aufzeichnung noch nicht verfügbar. Die Roh-Replay-Daten werden unten angezeigt.
replays-detail-raw-payload = Rohdaten
replays-related-errors = Fehler in diesem Replay
replays-col-level = Level
replays-col-title = Titel

# --- Seitennavigation ---
replays-pagination-label = Seitennavigation
replays-pagination-prev = « Zurück
replays-pagination-next = Weiter »
replays-count = { $count ->
    [one] { $count } Replay
   *[other] { $count } Replays
}
