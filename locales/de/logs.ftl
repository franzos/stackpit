# Log-Oberfläche: die projektbezogene Log-Liste. Nutzt nav-logs wieder.
# Zählstrings nutzen tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix ---
logs-title-suffix = — Stackpit

# --- Log-Liste ---
logs-list-search-placeholder = Logs durchsuchen…
logs-list-search-label = Logs durchsuchen
logs-list-filter-level = Nach Level filtern
logs-list-level-all = Alle Level
logs-filter-submit = Filtern
logs-list-empty = Keine Logs entsprechen den aktuellen Filtern.
logs-col-timestamp = Zeitstempel
logs-col-level = Level
logs-col-body = Nachricht
logs-col-trace = Trace
logs-col-release = Release
logs-body-empty = (leer)

# --- Seitennavigation ---
logs-pagination-label = Seitennavigation
logs-pagination-prev = « Zurück
logs-pagination-next = Weiter »
logs-count = { $count ->
    [one] { $count } Log
   *[other] { $count } Logs
}
