# Monitor-Oberfläche: die projektbezogene Monitor-Liste (Cron-Check-ins) und
# die Monitor-Detailseite. Nutzt nav-monitors wieder. Zählstrings nutzen
# tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix ---
monitors-title-suffix = — Stackpit

# --- Monitor-Liste ---
monitors-list-empty = Keine Monitore gefunden. Check-in-Ereignisse mit einem <code class="text-mono">monitor_slug</code> erscheinen hier.
monitors-col-slug = Slug
monitors-col-last-status = Letzter Status
monitors-col-last-checkin = Letzter Check-in
monitors-col-count = Anzahl

# --- Monitor-Detail ---
monitors-detail-title-prefix = Monitor
monitors-detail-subtitle = Monitor-Check-ins.
monitors-detail-empty = Keine Check-ins für diesen Monitor gefunden.
monitors-detail-select-checkin = Check-in auswählen
monitors-detail-confirm-delete-selected = Ausgewählte Check-ins löschen?
monitors-detail-delete = Löschen
monitors-detail-col-title = Titel
monitors-detail-col-level = Level
monitors-detail-col-environment = Umgebung
monitors-detail-col-time = Zeit
monitors-detail-untitled = (ohne Titel)
monitors-detail-confirm-delete-all = { $count ->
    [one] Alle { $count } Check-ins löschen?
   *[other] Alle { $count } Check-ins löschen?
}
monitors-detail-delete-all = { $count ->
    [one] Alle { $count } löschen
   *[other] Alle { $count } löschen
}

# --- Seitennavigation ---
monitors-pagination-label = Seitennavigation
monitors-pagination-prev = « Zurück
monitors-pagination-next = Weiter »
monitors-detail-count = { $count ->
    [one] { $count } Check-in
   *[other] { $count } Check-ins
}
