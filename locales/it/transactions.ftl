# Superficie transazioni: l'elenco transazioni per progetto e la pagina di
# dettaglio (istanze). Usa nav-transactions. Le stringhe conteggiate usano i
# plurali tv_count ([one]/[other]).

# --- Suffisso del titolo (titoli con prefisso dinamico) ---
transactions-title-suffix = — Stackpit

# --- Elenco transazioni ---
transactions-time-range = Intervallo di tempo
transactions-filter-submit = Filtra
transactions-list-empty = Nessuna transazione in questo periodo.
transactions-col-name = Transazione
transactions-col-throughput = Throughput
transactions-col-failure = % errori
transactions-col-count = Conteggio
transactions-col-users = Utenti

# --- Dettaglio transazione (istanze) ---
transactions-detail-op = op:
transactions-detail-empty = Nessuna istanza registrata per questa transazione.
transactions-detail-col-duration = Durata
transactions-detail-col-status = Stato
transactions-detail-col-trace = Trace
transactions-detail-col-when = Quando
transactions-detail-distribution = Distribuzione delle durate
transactions-detail-spans = Ripartizione degli span
transactions-detail-issues = Problemi correlati
transactions-detail-instances = Istanze più lente
transactions-detail-trend = Andamento dei percentili
transactions-detail-trend-note = I punti evidenziati sono quelli in cui il p95 ha superato di 1,5 volte la mediana dei cinque punti precedenti.

# --- Impaginazione (dettaglio transazione) ---
transactions-pagination-label = Impaginazione
transactions-pagination-prev = « Precedente
transactions-pagination-next = Successivo »
transactions-detail-count = { $count ->
    [one] { $count } istanza
   *[other] { $count } istanze
}
