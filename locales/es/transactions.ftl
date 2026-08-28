# Superficie de transacciones: la lista de transacciones por proyecto y la página
# de detalle (instancias). Reutiliza nav-transactions. Los strings contados usan
# plurales tv_count ([one]/[other]).

# --- Sufijo de título (títulos con prefijo dinámico) ---
transactions-title-suffix = — Stackpit

# --- Lista de transacciones ---
transactions-time-range = Rango de tiempo
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
transactions-detail-spans = Desglose de spans
transactions-detail-issues = Problemas relacionados
transactions-detail-instances = Instancias más lentas
transactions-detail-trend = Tendencia de percentiles
transactions-detail-trend-note = Los puntos marcados son aquellos en los que el p95 superó 1,5 veces la mediana de los cinco puntos anteriores.

# --- Paginación (detalle de transacción) ---
transactions-pagination-label = Paginación
transactions-pagination-prev = « Anterior
transactions-pagination-next = Siguiente »
transactions-detail-count = { $count ->
    [one] { $count } instancia
   *[other] { $count } instancias
}
transactions-detail-failure-label = Fallos
