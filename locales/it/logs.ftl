# Superficie log: l'elenco log per progetto. Usa nav-logs. Le stringhe
# conteggiate usano i plurali tv_count ([one]/[other]).

# --- Suffisso del titolo ---
logs-title-suffix = — Stackpit

# --- Elenco log ---
logs-list-search-placeholder = Cerca nei log…
logs-list-search-label = Cerca nei log
logs-list-filter-level = Filtra per livello
logs-list-level-all = Tutti i livelli
logs-filter-submit = Filtra
logs-list-empty = Nessun log corrisponde ai filtri attuali.
logs-col-timestamp = Timestamp
logs-col-level = Livello
logs-col-body = Corpo
logs-col-trace = Trace
logs-col-release = Release
logs-body-empty = (vuoto)

# --- Impaginazione ---
logs-pagination-label = Impaginazione
logs-pagination-prev = « Precedente
logs-pagination-next = Successivo »
logs-count = { $count ->
    [one] { $count } log
   *[other] { $count } log
}
