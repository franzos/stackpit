# Superficie de transacciones: la lista de transacciones por proyecto y la página
# de detalle (instancias). Reutiliza nav-transactions. Los strings contados usan
# plurales tv_count ([one]/[other]).

# --- Sufijo de título (títulos con prefijo dinámico) ---
transactions-title-suffix = — Stackpit

# --- Lista de transacciones ---
transactions-time-range = Rango de tiempo
transactions-period-1h = Última hora
transactions-period-24h = Últimas 24 h
transactions-period-7d = Últimos 7 días
transactions-period-14d = Últimos 14 días
transactions-period-30d = Últimos 30 días
transactions-period-90d = Últimos 90 días
transactions-filter-submit = Filtrar
transactions-list-empty = No hay transacciones en este periodo.
transactions-col-name = Transacción
transactions-col-throughput = Rendimiento
transactions-col-failure = % de fallos
transactions-col-count = Recuento
transactions-col-users = Usuarios

# --- Detalle de transacción (instancias) ---
transactions-detail-op = op:
transactions-detail-empty = No se registraron instancias para esta transacción.
transactions-detail-col-duration = Duración
transactions-detail-col-status = Estado
transactions-detail-col-trace = Trace
transactions-detail-col-when = Cuándo
transactions-detail-distribution = Distribución de duración

# --- Paginación (detalle de transacción) ---
transactions-pagination-label = Paginación
transactions-pagination-prev = « Anterior
transactions-pagination-next = Siguiente »
transactions-detail-count = { $count ->
    [one] { $count } instancia
   *[other] { $count } instancias
}
