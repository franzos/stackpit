# Superficie de monitores: la lista de monitores (check-ins de cron) por proyecto
# y la página de detalle del monitor. Reutiliza nav-monitors. Los strings contados
# usan plurales tv_count ([one]/[other]).

# --- Sufijo de título ---
monitors-title-suffix = — Stackpit

# --- Lista de monitores ---
monitors-list-empty = No se encontraron monitores. Los eventos de check-in con un <code class="text-mono">monitor_slug</code> aparecerán aquí.
monitors-col-slug = Slug
monitors-col-last-status = Último estado
monitors-col-last-checkin = Último check-in
monitors-col-count = Recuento

# --- Detalle del monitor ---
monitors-detail-title-prefix = Monitor
monitors-detail-subtitle = Check-ins del monitor.
monitors-detail-empty = No se encontraron check-ins para este monitor.
monitors-detail-select-checkin = Seleccionar check-in
monitors-detail-confirm-delete-selected = ¿Eliminar los check-ins seleccionados?
monitors-detail-delete = Eliminar
monitors-detail-col-title = Título
monitors-detail-col-level = Nivel
monitors-detail-col-environment = Entorno
monitors-detail-col-time = Hora
monitors-detail-untitled = (sin título)
monitors-detail-confirm-delete-all = { $count ->
    [one] ¿Eliminar el { $count } check-in?
   *[other] ¿Eliminar los { $count } check-ins?
}
monitors-detail-delete-all = { $count ->
    [one] Eliminar el { $count }
   *[other] Eliminar los { $count }
}

# --- Paginación ---
monitors-pagination-label = Paginación
monitors-pagination-prev = « Anterior
monitors-pagination-next = Siguiente »
monitors-detail-count = { $count ->
    [one] { $count } check-in
   *[other] { $count } check-ins
}
