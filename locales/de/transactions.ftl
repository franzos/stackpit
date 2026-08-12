# Transaktions-Oberfläche: die projektbezogene Transaktionsliste und die
# Transaktions-Detailseite (Instanzen). Nutzt nav-transactions wieder.
# Zählstrings nutzen tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix (Titel mit dynamischem Präfix) ---
transactions-title-suffix = — Stackpit

# --- Transaktionsliste ---
transactions-time-range = Zeitraum
transactions-filter-submit = Filtern
transactions-list-empty = Keine Transaktionen in diesem Zeitraum.
transactions-col-name = Transaktion
transactions-col-throughput = Durchsatz
transactions-col-failure = Fehler %
transactions-col-count = Anzahl
transactions-col-users = Nutzer

# --- Transaktionsdetail (Instanzen) ---
transactions-detail-op = op:
transactions-detail-empty = Keine Instanzen für diese Transaktion aufgezeichnet.
transactions-detail-col-duration = Dauer
transactions-detail-col-status = Status
transactions-detail-col-trace = Trace
transactions-detail-col-when = Wann
transactions-detail-distribution = Dauerverteilung
transactions-detail-spans = Span-Aufschlüsselung
transactions-detail-issues = Zugehörige Probleme
transactions-detail-instances = Langsamste Instanzen
transactions-detail-trend = Perzentil-Verlauf
transactions-detail-trend-note = Markierte Punkte sind Stellen, an denen p95 den Median der fünf vorherigen Punkte um mehr als das 1,5-Fache überschritten hat.

# --- Seitennavigation (Transaktionsdetail) ---
transactions-pagination-label = Seitennavigation
transactions-pagination-prev = « Zurück
transactions-pagination-next = Weiter »
transactions-detail-count = { $count ->
    [one] { $count } Instanz
   *[other] { $count } Instanzen
}
