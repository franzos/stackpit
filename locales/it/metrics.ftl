# Superficie metriche: l'elenco metriche per progetto e la pagina di dettaglio
# della serie temporale. Usa nav-metrics. Le stringhe conteggiate usano i
# plurali tv_count ([one]/[other]).

# --- Suffisso del titolo ---
metrics-title-suffix = — Stackpit

# --- Elenco metriche ---
metrics-list-empty = Nessuna metrica trovata. Gli eventi delle metriche appariranno qui una volta ricevuti.
metrics-col-mri = MRI
metrics-col-type = Tipo
metrics-col-data-points = Punti dati
metrics-col-first-seen = Prima occorrenza
metrics-col-last-seen = Ultima occorrenza

# --- Impaginazione ---
metrics-pagination-label = Impaginazione
metrics-pagination-prev = « Precedente
metrics-pagination-next = Successivo »
metrics-count = { $count ->
    [one] { $count } metrica
   *[other] { $count } metriche
}

# --- Dettaglio metrica (intervalli orari) ---
metrics-detail-empty = Nessun punto dati nell'intervallo di tempo selezionato.
metrics-detail-col-time = Ora (intervallo orario)
metrics-detail-col-count = Conteggio
metrics-detail-col-sum = Somma
metrics-detail-col-min = Min
metrics-detail-col-max = Max
metrics-detail-col-avg = Media
