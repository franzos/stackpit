# Metrik-Oberfläche: die projektbezogene Metrikliste und die Detailseite mit
# der Metrik-Zeitreihe. Nutzt nav-metrics wieder. Zählstrings nutzen
# tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix ---
metrics-title-suffix = — Stackpit

# --- Metrikliste ---
metrics-list-empty = Keine Metriken gefunden. Metrik-Ereignisse erscheinen hier, sobald sie empfangen werden.
metrics-col-mri = MRI
metrics-col-type = Typ
metrics-col-data-points = Datenpunkte
metrics-col-first-seen = Zuerst gesehen
metrics-col-last-seen = Zuletzt gesehen

# --- Seitennavigation ---
metrics-pagination-label = Seitennavigation
metrics-pagination-prev = « Zurück
metrics-pagination-next = Weiter »
metrics-count = { $count ->
    [one] { $count } Metrik
   *[other] { $count } Metriken
}

# --- Metrikdetail (Stundenintervalle) ---
metrics-detail-empty = Keine Datenpunkte im gewählten Zeitraum.
metrics-detail-col-time = Zeit (Stundenintervall)
metrics-detail-col-count = Anzahl
metrics-detail-col-sum = Summe
metrics-detail-col-min = Min
metrics-detail-col-max = Max
metrics-detail-col-avg = Ø
