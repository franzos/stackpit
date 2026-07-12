# Span-Oberfläche: die projektbezogene Span-/Trace-Liste (spans-*) und die
# Trace-Wasserfall-Detailseite (trace-detail-*). Nutzt nav-spans wieder.
# Zählstrings nutzen tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix ---
spans-title-suffix = — Stackpit

# --- Span-/Trace-Liste ---
spans-list-empty = Keine Spans für dieses Projekt gefunden.
spans-traces-heading = Traces
spans-all-heading = Alle Spans

# --- Trace-Tabelle ---
spans-col-trace-id = Trace-ID
spans-col-root-op = Root-Op
spans-col-root-description = Root-Beschreibung
spans-col-duration = Dauer
spans-col-first-seen = Zuerst gesehen
spans-col-last-seen = Zuletzt gesehen

# --- Alle-Spans-Tabelle ---
spans-col-span-id = Span-ID
spans-col-op = Op
spans-col-description = Beschreibung
spans-col-timestamp = Zeitstempel

# --- Seitennavigation (Span-Liste) ---
spans-pagination-label = Seitennavigation
spans-pagination-prev = « Zurück
spans-pagination-next = Weiter »
spans-count = { $count ->
    [one] { $count } Span
   *[other] { $count } Spans
}

# --- Trace-Detail (Wasserfall) ---
trace-detail-title-prefix = Trace
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = gesamt
trace-detail-showing-first = zeige die ersten
trace-detail-of = von
trace-detail-empty = Keine Spans für diesen Trace gefunden.
trace-detail-col-span = Span
trace-detail-col-duration = Dauer
trace-detail-root-fallback = (Trace-Root)
trace-detail-error-title = Fehler
trace-detail-span-fallback = Span
trace-detail-compressed-note = Leerlauf-Lücken komprimiert
trace-detail-gap-title = Zusammengefasste Leerlauf-Lücke (keine aktiven Spans)
trace-detail-lbl-span-id = Span-ID
trace-detail-lbl-parent = Übergeordneter Span
trace-detail-lbl-status = Status
trace-detail-lbl-start = Start-Offset
trace-detail-correlated-errors = Korrelierte Fehler
trace-detail-col-level = Level
trace-detail-col-title = Titel
trace-detail-col-timestamp = Zeitstempel
trace-detail-span-count = { $count ->
    [one] { $count } Span
   *[other] { $count } Spans
}
