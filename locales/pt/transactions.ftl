# Interface de transações: a lista de transações por projeto e a página de
# detalhe da transação (instâncias). Reutiliza nav-transactions. As strings com
# contagem usam plurais tv_count ([one]/[other]).

# --- Sufixo do título da página (títulos com prefixo dinâmico) ---
transactions-title-suffix = — Stackpit

# --- Lista de transações ---
transactions-time-range = Intervalo de tempo
transactions-period-1h = Última hora
transactions-period-24h = Últimas 24h
transactions-period-7d = Últimos 7 dias
transactions-period-14d = Últimos 14 dias
transactions-period-30d = Últimos 30 dias
transactions-period-90d = Últimos 90 dias
transactions-filter-submit = Filtrar
transactions-list-empty = Nenhuma transação neste período.
transactions-col-name = Transação
transactions-col-throughput = Débito
transactions-col-failure = Falhas %
transactions-col-count = Contagem
transactions-col-users = Utilizadores

# --- Detalhe da transação (instâncias) ---
transactions-detail-op = op:
transactions-detail-empty = Nenhuma instância registada para esta transação.
transactions-detail-col-duration = Duração
transactions-detail-col-status = Estado
transactions-detail-col-trace = Trace
transactions-detail-col-when = Quando
transactions-detail-distribution = Distribuição de duração

# --- Paginação (detalhe da transação) ---
transactions-pagination-label = Paginação
transactions-pagination-prev = « Anterior
transactions-pagination-next = Seguinte »
transactions-detail-count = { $count ->
    [one] { $count } instância
   *[other] { $count } instâncias
}
