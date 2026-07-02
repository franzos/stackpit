# Superficie monitor: l'elenco monitor (check-in cron) per progetto e la pagina
# di dettaglio. Usa nav-monitors. Le stringhe conteggiate usano i plurali
# tv_count ([one]/[other]).

# --- Suffisso del titolo ---
monitors-title-suffix = — Stackpit

# --- Elenco monitor ---
monitors-list-empty = Nessun monitor trovato. Gli eventi di check-in con un <code class="text-mono">monitor_slug</code> appariranno qui.
monitors-col-slug = Slug
monitors-col-last-status = Ultimo stato
monitors-col-last-checkin = Ultimo check-in
monitors-col-count = Conteggio

# --- Dettaglio monitor ---
monitors-detail-title-prefix = Monitor
monitors-detail-subtitle = Check-in del monitor.
monitors-detail-empty = Nessun check-in trovato per questo monitor.
monitors-detail-select-checkin = Seleziona check-in
monitors-detail-confirm-delete-selected = Eliminare i check-in selezionati?
monitors-detail-delete = Elimina
monitors-detail-col-title = Titolo
monitors-detail-col-level = Livello
monitors-detail-col-environment = Ambiente
monitors-detail-col-time = Ora
monitors-detail-untitled = (senza titolo)
monitors-detail-confirm-delete-all = { $count ->
    [one] Eliminare { $count } check-in?
   *[other] Eliminare tutti i { $count } check-in?
}
monitors-detail-delete-all = { $count ->
    [one] Elimina { $count }
   *[other] Elimina tutti i { $count }
}

# --- Impaginazione ---
monitors-pagination-label = Impaginazione
monitors-pagination-prev = « Precedente
monitors-pagination-next = Successivo »
monitors-detail-count = { $count ->
    [one] { $count } check-in
   *[other] { $count } check-in
}
