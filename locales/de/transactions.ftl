# Transaktions-Oberfläche: die projektbezogene Transaktionsliste und die
# Transaktions-Detailseite (Instanzen). Nutzt nav-transactions wieder.
# Zählstrings nutzen tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix (Titel mit dynamischem Präfix) ---
transactions-title-suffix = — Stackpit

# --- Transaktionsliste ---
transactions-time-range = Zeitraum
transactions-period-1h = Letzte Stunde
transactions-period-24h = Letzte 24 Std.
transactions-period-7d = Letzte 7 Tage
transactions-period-14d = Letzte 14 Tage
transactions-period-30d = Letzte 30 Tage
transactions-period-90d = Letzte 90 Tage
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

# --- Seitennavigation (Transaktionsdetail) ---
transactions-pagination-label = Seitennavigation
transactions-pagination-prev = « Zurück
transactions-pagination-next = Weiter »
transactions-detail-count = { $count ->
    [one] { $count } Instanz
   *[other] { $count } Instanzen
}
