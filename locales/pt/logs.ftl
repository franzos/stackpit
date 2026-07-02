# Interface de registos: a lista de registos por projeto. Reutiliza nav-logs.
# As strings com contagem usam plurais tv_count ([one]/[other]).

# --- Sufixo do título da página ---
logs-title-suffix = — Stackpit

# --- Lista de registos ---
logs-list-search-placeholder = Pesquisar registos…
logs-list-search-label = Pesquisar registos
logs-list-filter-level = Filtrar por nível
logs-list-level-all = Todos os níveis
logs-filter-submit = Filtrar
logs-list-empty = Nenhum registo corresponde aos filtros atuais.
logs-col-timestamp = Data/hora
logs-col-level = Nível
logs-col-body = Corpo
logs-col-trace = Trace
logs-col-release = Release
logs-body-empty = (vazio)

# --- Paginação ---
logs-pagination-label = Paginação
logs-pagination-prev = « Anterior
logs-pagination-next = Seguinte »
logs-count = { $count ->
    [one] { $count } registo
   *[other] { $count } registos
}
