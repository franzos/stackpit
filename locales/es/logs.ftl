# Superficie de registros: la lista de registros por proyecto. Reutiliza nav-logs.
# Los strings contados usan plurales tv_count ([one]/[other]).

# --- Sufijo de título ---
logs-title-suffix = — Stackpit

# --- Lista de registros ---
logs-list-search-placeholder = Buscar registros…
logs-list-search-label = Buscar registros
logs-list-filter-level = Filtrar por nivel
logs-list-level-all = Todos los niveles
logs-filter-submit = Filtrar
logs-list-empty = Ningún registro coincide con los filtros actuales.
logs-col-timestamp = Marca de tiempo
logs-col-level = Nivel
logs-col-body = Cuerpo
logs-col-trace = Trace
logs-col-release = Release
logs-body-empty = (vacío)

# --- Paginación ---
logs-pagination-label = Paginación
logs-pagination-prev = « Anterior
logs-pagination-next = Siguiente »
logs-count = { $count ->
    [one] { $count } registro
   *[other] { $count } registros
}
