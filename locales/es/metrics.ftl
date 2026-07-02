# Superficie de métricas: la lista de métricas por proyecto y la página de
# detalle de la serie de métricas. Reutiliza nav-metrics. Los strings contados
# usan plurales tv_count ([one]/[other]).

# --- Sufijo de título ---
metrics-title-suffix = — Stackpit

# --- Lista de métricas ---
metrics-list-empty = No se encontraron métricas. Los eventos de métricas aparecerán aquí una vez recibidos.
metrics-col-mri = MRI
metrics-col-type = Tipo
metrics-col-data-points = Puntos de datos
metrics-col-first-seen = Visto por primera vez
metrics-col-last-seen = Visto por última vez

# --- Paginación ---
metrics-pagination-label = Paginación
metrics-pagination-prev = « Anterior
metrics-pagination-next = Siguiente »
metrics-count = { $count ->
    [one] { $count } métrica
   *[other] { $count } métricas
}

# --- Detalle de métrica (intervalos horarios) ---
metrics-detail-empty = No hay puntos de datos en el rango de tiempo seleccionado.
metrics-detail-col-time = Hora (intervalo horario)
metrics-detail-col-count = Recuento
metrics-detail-col-sum = Suma
metrics-detail-col-min = Mín
metrics-detail-col-max = Máx
metrics-detail-col-avg = Prom
