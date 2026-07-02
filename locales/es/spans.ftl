# Superficie de spans: la lista de spans/traces por proyecto (spans-*) y la
# página de detalle de trace en cascada (trace-detail-*). Reutiliza nav-spans.
# Los strings contados usan plurales tv_count ([one]/[other]).

# --- Sufijo de título ---
spans-title-suffix = — Stackpit

# --- Lista de spans/traces ---
spans-list-empty = No se encontraron spans para este proyecto.
spans-traces-heading = Traces
spans-all-heading = Todos los spans

# --- Tabla de traces ---
spans-col-trace-id = ID de trace
spans-col-root-op = Op raíz
spans-col-root-description = Descripción raíz
spans-col-duration = Duración
spans-col-first-seen = Visto por primera vez
spans-col-last-seen = Visto por última vez

# --- Tabla de todos los spans ---
spans-col-span-id = ID de span
spans-col-op = Op
spans-col-description = Descripción
spans-col-timestamp = Marca de tiempo

# --- Paginación (lista de spans) ---
spans-pagination-label = Paginación
spans-pagination-prev = « Anterior
spans-pagination-next = Siguiente »
spans-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}

# --- Detalle de trace (cascada) ---
# title-prefix/suffix envuelven el trace id dinámico; total/showing-first/of se
# dividen en los límites de { $var } de la línea de metadatos.
trace-detail-title-prefix = Trace
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = en total
trace-detail-showing-first = mostrando los primeros
trace-detail-of = de
trace-detail-empty = No se encontraron spans para este trace.
trace-detail-col-span = Span
trace-detail-col-duration = Duración
trace-detail-root-fallback = (raíz del trace)
trace-detail-error-title = error
trace-detail-span-fallback = span
trace-detail-correlated-errors = Errores correlacionados
trace-detail-col-level = Nivel
trace-detail-col-title = Título
trace-detail-col-timestamp = Marca de tiempo
trace-detail-span-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}
