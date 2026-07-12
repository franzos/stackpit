# Fehler-Oberfläche: die nach Fingerabdruck gruppierte Fehlerliste und die
# Fehler-Detailseite. issue-detail-exception-stacktrace enthält ein Inline-&amp;
# und wird mit |safe gerendert. Zählstrings nutzen tv_count-Plurale.

# --- Gemeinsame Labels (Fehlerliste + Fehlerdetail) ---
issues-label-title = Titel
issues-label-level = Level
issues-label-events = Ereignisse
issues-label-users = Nutzer
issues-label-status = Status
issues-label-first-seen = Zuerst gesehen
issues-label-last-seen = Zuletzt gesehen
issues-label-value = Wert

# --- Status-Werte (Filteroptionen + Badges) ---
issues-status-unresolved = Ungelöst
issues-status-resolved = Gelöst
issues-status-ignored = Ignoriert

# --- Seitennavigation (gemeinsam) ---
issues-pagination-label = Seitennavigation
issues-pagination-prev = « Zurück
issues-pagination-next = Weiter »

# --- Titel-Suffix (Titel mit dynamischem Präfix) ---
issues-title-suffix = — Stackpit

# --- Fehlerliste ---
issues-list-subtitle = Fehler nach Fingerabdruck gruppiert.
issues-list-filtered-by-tag = Gefiltert nach Tag:
issues-list-clear-tag = Tag-Filter entfernen
issues-list-search-placeholder = Fehler durchsuchen…
issues-list-search-label = Fehler durchsuchen
issues-list-select = Fehler auswählen
issues-list-filter-status = Nach Status filtern
issues-list-status-all = Alle Status
issues-list-filter-level = Nach Level filtern
issues-list-level-all = Alle Level
issues-list-filter-release = Nach Release filtern
issues-list-release-all = Alle Releases
issues-period-label = Zeitraum
issues-period-all = Gesamter Zeitraum
issues-period-1h = Letzte Stunde
issues-period-24h = Letzte 24 Std.
issues-period-7d = Letzte 7 Tage
issues-period-14d = Letzte 14 Tage
issues-period-30d = Letzte 30 Tage
issues-period-90d = Letzte 90 Tage
issues-period-365d = Letzte 365 Tage
issues-list-filter-submit = Filtern
issues-list-empty = Keine Fehler entsprechen den aktuellen Filtern.
issues-untitled = (ohne Titel)

# --- Massenaktionen ---
issues-bulk-resolve-all = Alle { $count } lösen
issues-bulk-ignore-all = Alle { $count } ignorieren
issues-bulk-delete-all = Alle { $count } löschen
issues-bulk-resolve-confirm = { $count ->
    [one] Alle { $count } passenden Fehler beheben?
   *[other] Alle { $count } passenden Fehler beheben?
}
issues-bulk-ignore-confirm = { $count ->
    [one] Alle { $count } passenden Fehler ignorieren?
   *[other] Alle { $count } passenden Fehler ignorieren?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] Alle { $count } passenden Fehler dauerhaft löschen?
   *[other] Alle { $count } passenden Fehler dauerhaft löschen?
}
issues-bulk-resolve = Lösen
issues-bulk-ignore = Ignorieren
issues-bulk-delete = Löschen
issues-bulk-delete-selected-confirm = Ausgewählte Fehler dauerhaft löschen?

# --- Anzahl (Seitennavigation) ---
issues-count = { $count ->
    [one] { $count } Fehler
   *[other] { $count } Fehler
}

# --- Fehlerdetail ---
issue-detail-title-fallback = Fehler
issue-detail-resolve = ✓ Lösen
issue-detail-reopen = Wieder öffnen
issue-detail-unignore = Nicht mehr ignorieren
issue-detail-tab-details = Details
issue-detail-tab-events = Alle Ereignisse
issue-detail-exception-stacktrace = Ausnahme &amp; Stacktrace
issue-detail-handled = behandelt
issue-detail-unhandled = unbehandelt
issue-detail-in = in
issue-detail-var-name = Variable
issue-detail-no-source = Kein Quellcode-Kontext verfügbar
issue-detail-minified-hint = Diese Frames wirken minifiziert und es wurde keine Sourcemap angewendet.
issue-detail-minified-hint-link = Sourcemaps hochladen
issue-detail-breadcrumbs = Navigationspfad
issue-detail-th-time = Zeit
issue-detail-th-category = Kategorie
issue-detail-th-message = Nachricht
issue-detail-crumb-data = Daten
issue-detail-tags = Tags
issue-detail-contexts = Kontexte
issue-detail-request = Anfrage
issue-detail-headers = Header
issue-detail-th-header = Header
issue-detail-query-string = Query-String
issue-detail-body = Inhalt
issue-detail-environment = Umgebung
issue-detail-user-reports = Nutzerberichte
issue-detail-anonymous = Anonym
issue-detail-attachments = Anhänge
issue-detail-att-filename = Dateiname
issue-detail-att-type = Typ
issue-detail-att-size = Größe
issue-detail-download = Herunterladen
issue-detail-raw-json = Roh-JSON
issue-detail-no-events = Keine Ereignisse für diesen Fehler gefunden.
issue-detail-ev-id = Ereignis-ID
issue-detail-ev-timestamp = Zeitstempel
issue-detail-ev-platform = Plattform
issue-detail-events-count = { $count ->
    [one] { $count } Ereignis
   *[other] { $count } Ereignisse
}
issue-detail-props-heading = Fehlereigenschaften
issue-detail-fingerprint = Fingerabdruck
issue-detail-tag-facets = Tag-Facetten
issue-detail-discard-undo-title = Zukünftige Ereignisse mit diesem Fingerabdruck wieder annehmen
issue-detail-discard-undo = Verwerfen rückgängig machen
issue-detail-discard-confirm = Alle zukünftigen Ereignisse mit diesem Fingerabdruck verwerfen?
issue-detail-discard-title = Zukünftige Ereignisse mit diesem Fingerabdruck stillschweigend verwerfen
issue-detail-discard = Zukünftige Ereignisse verwerfen
