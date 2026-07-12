# Ereignis-Oberfläche: die projektübergreifende Ereignisliste und die
# Ereignis-Detailseite. event-detail-exception-stacktrace enthält ein Inline-&amp;
# und wird mit |safe gerendert. Zählstrings nutzen tv_count-Plurale.

# --- Gemeinsame Labels (Ereignisliste + Ereignisdetail) ---
events-label-title = Titel
events-label-type = Typ
events-label-level = Level
events-label-platform = Plattform
events-label-environment = Umgebung
events-label-time = Zeit
events-label-value = Wert

# --- Seitennavigation (gemeinsam) ---
events-pagination-label = Seitennavigation
events-pagination-prev = « Zurück
events-pagination-next = Weiter »

# --- Titel-Suffix (Titel mit dynamischem Präfix) ---
events-title-suffix = — Stackpit

# --- Ereignisliste ---
events-list-title = Ereignisse — Stackpit
events-heading = Ereignisse
events-list-search-placeholder = Ereignisse durchsuchen…
events-list-search-label = Ereignisse durchsuchen
events-list-select = Ereignis auswählen
events-list-filter-level = Nach Level filtern
events-list-level-all = Alle Level
events-list-filter-type = Nach Typ filtern
events-list-type-all = Alle Typen
events-list-project-placeholder = Projekt-ID
events-list-filter-project = Nach Projekt filtern
events-list-filter-submit = Filtern
events-list-empty = Keine Ereignisse entsprechen den aktuellen Filtern.
events-untitled = (ohne Titel)
events-col-project = Projekt

# --- Massenaktionen ---
events-bulk-delete = Löschen
events-bulk-delete-selected-confirm = Ausgewählte Ereignisse löschen?
events-bulk-delete-all = Alle { $count } passenden löschen
events-bulk-delete-all-confirm = { $count ->
    [one] Alle { $count } passenden Ereignisse dauerhaft löschen?
   *[other] Alle { $count } passenden Ereignisse dauerhaft löschen?
}

# --- Anzahl (Seitennavigation) ---
events-count = { $count ->
    [one] { $count } Ereignis
   *[other] { $count } Ereignisse
}

# --- Ereignisdetail ---
event-detail-event = Ereignis
event-detail-event-id-label = event_id:
event-detail-nav-label = Ereignis-Navigation
event-detail-nav-newer = « Neuer
event-detail-nav-older = Älter »
event-detail-nav-count = { $count ->
    [one] { $count } Ereignis
   *[other] { $count } Ereignisse
}
event-detail-nav-in-issue = im Fehler
event-detail-user-feedback = Nutzer-Feedback
event-detail-anonymous = Anonym
event-detail-related-event = Verbundenes Ereignis:
event-detail-exception-stacktrace = Ausnahme &amp; Stacktrace
event-detail-handled = behandelt
event-detail-unhandled = unbehandelt
event-detail-in = in
event-detail-var-name = Variable
event-detail-no-source = Kein Quellcode-Kontext verfügbar
event-detail-breadcrumbs = Navigationspfad
event-detail-th-category = Kategorie
event-detail-th-message = Nachricht
event-detail-tags = Tags
event-detail-contexts = Kontexte
event-detail-request = Anfrage
event-detail-headers = Header
event-detail-th-header = Header
event-detail-query-string = Query-String
event-detail-body = Inhalt
event-detail-user-reports = Nutzerberichte
event-detail-attachments = Anhänge
event-detail-att-filename = Dateiname
event-detail-att-size = Größe
event-detail-download = Herunterladen
event-detail-web-vitals = Web Vitals
event-detail-raw-json = Roh-JSON
event-detail-props-heading = Ereigniseigenschaften
event-detail-prop-event-id = Ereignis-ID
event-detail-prop-timestamp = Zeitstempel
event-detail-prop-transaction = Transaktion
event-detail-prop-release = Release
event-detail-prop-server = Server
event-detail-prop-sdk = SDK
event-detail-prop-received = Empfangen
event-detail-user-heading = Nutzer
event-detail-user-id = ID
event-detail-user-email = E-Mail
event-detail-user-username = Nutzername
event-detail-user-ip = IP-Adresse

# --- Client-Berichte (verworfene Events) ---
# Nutzt events-untitled und events-pagination-* (gemeinsam, gleiche Datei).
client-reports-title = Client-Berichte
client-reports-heading = Client-Berichte
client-reports-dropped-heading = Verworfene Events
client-reports-dropped-subtitle = Was die SDKs vor dem Senden verworfen haben, nach Kategorie und Grund.
client-reports-th-category = Kategorie
client-reports-th-reason = Grund
client-reports-th-reasons = Gründe
client-reports-th-dropped = Verworfen
client-reports-empty = Keine Client-Berichte für dieses Projekt gefunden.
client-reports-reports-heading = Berichte
client-reports-delete = Löschen
client-reports-delete-selected-confirm = Ausgewählte Berichte löschen?
client-reports-th-event-id = Ereignis-ID
client-reports-th-title = Titel
client-reports-th-timestamp = Zeitstempel
client-reports-th-platform = Plattform
client-reports-th-release = Release
client-reports-select = Bericht auswählen
client-reports-delete-all = Alle { $count } löschen
client-reports-delete-all-confirm = { $count ->
    [one] Diesen passenden Bericht löschen?
   *[other] Alle { $count } passenden Berichte löschen?
}
client-reports-count = { $count ->
    [one] { $count } Bericht
   *[other] { $count } Berichte
}

# --- Nutzerberichte (Nutzer-Feedback) ---
# Nutzt events-untitled und events-pagination-* (gemeinsam, gleiche Datei).
user-reports-title = Nutzerberichte
user-reports-heading = Nutzerberichte
user-reports-empty = Keine Nutzerberichte für dieses Projekt gefunden.
user-reports-delete = Löschen
user-reports-delete-selected-confirm = Ausgewählte Berichte löschen?
user-reports-th-event-id = Ereignis-ID
user-reports-th-title = Titel
user-reports-th-timestamp = Zeitstempel
user-reports-th-platform = Plattform
user-reports-th-release = Release
user-reports-select = Bericht auswählen
user-reports-delete-all = Alle { $count } löschen
user-reports-delete-all-confirm = { $count ->
    [one] Diesen passenden Bericht löschen?
   *[other] Alle { $count } passenden Berichte löschen?
}
user-reports-count = { $count ->
    [one] { $count } Bericht
   *[other] { $count } Berichte
}
