# Superficie de problemas: la lista de problemas agrupados por fingerprint y la
# página de detalle. issue-detail-exception-stacktrace contiene un &amp; en línea
# y se renderiza con |safe. Los strings contados usan plurales tv_count ([one]/[other]).

# --- Labels compartidos (lista de problemas + detalle) ---
issues-label-title = Título
issues-label-level = Nivel
issues-label-events = Eventos
issues-label-users = Usuarios
issues-label-status = Estado
issues-label-first-seen = Visto por primera vez
issues-label-last-seen = Visto por última vez
issues-label-value = Valor

# --- Valores de estado (opciones de filtro + insignias) ---
issues-status-unresolved = Sin resolver
issues-status-resolved = Resuelto
issues-status-ignored = Ignorado

# --- Paginación (compartida) ---
issues-pagination-label = Paginación
issues-pagination-prev = « Anterior
issues-pagination-next = Siguiente »

# --- Sufijo de título (títulos con prefijo dinámico) ---
issues-title-suffix = — Stackpit

# --- Lista de problemas ---
issues-list-subtitle = Problemas agrupados por fingerprint.
issues-list-filtered-by-tag = Filtrado por etiqueta:
issues-list-clear-tag = Quitar filtro de etiqueta
issues-list-search-placeholder = Buscar problemas…
issues-list-search-label = Buscar problemas
issues-list-select = Seleccionar problema
issues-list-filter-status = Filtrar por estado
issues-list-status-all = Todos los estados
issues-list-filter-level = Filtrar por nivel
issues-list-level-all = Todos los niveles
issues-list-filter-release = Filtrar por release
issues-list-release-all = Todos los releases
issues-period-label = Rango de tiempo
issues-period-all = Todo el tiempo
issues-period-1h = Última hora
issues-period-24h = Últimas 24 h
issues-period-7d = Últimos 7 días
issues-period-14d = Últimos 14 días
issues-period-30d = Últimos 30 días
issues-period-90d = Últimos 90 días
issues-period-365d = Últimos 365 días
issues-list-filter-submit = Filtrar
issues-list-empty = Ningún problema coincide con los filtros actuales.
issues-untitled = (sin título)

# --- Acciones masivas ---
issues-bulk-resolve-all = Resolver los { $count }
issues-bulk-ignore-all = Ignorar los { $count }
issues-bulk-delete-all = Eliminar los { $count }
issues-bulk-resolve-confirm = { $count ->
    [one] ¿Resolver el { $count } problema coincidente?
   *[other] ¿Resolver los { $count } problemas coincidentes?
}
issues-bulk-ignore-confirm = { $count ->
    [one] ¿Ignorar el { $count } problema coincidente?
   *[other] ¿Ignorar los { $count } problemas coincidentes?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] ¿Eliminar permanentemente el { $count } problema coincidente?
   *[other] ¿Eliminar permanentemente los { $count } problemas coincidentes?
}
issues-bulk-resolve = Resolver
issues-bulk-ignore = Ignorar
issues-bulk-delete = Eliminar
issues-bulk-delete-selected-confirm = ¿Eliminar permanentemente los problemas seleccionados?

# --- Recuento (paginación) ---
issues-count = { $count ->
    [one] { $count } problema
   *[other] { $count } problemas
}

# --- Detalle del problema ---
issue-detail-title-fallback = Problema
issue-detail-resolve = ✓ Resolver
issue-detail-reopen = Reabrir
issue-detail-unignore = Dejar de ignorar
issue-detail-tab-details = Detalles
issue-detail-tab-events = Todos los eventos
issue-detail-exception-stacktrace = Excepción &amp; Stacktrace
issue-detail-handled = gestionado
issue-detail-unhandled = no gestionado
issue-detail-in = en
issue-detail-var-name = Variable
issue-detail-no-source = No hay contexto de código fuente disponible
issue-detail-minified-hint = Estos frames parecen minificados y no se aplicó ningún source map.
issue-detail-minified-hint-link = Subir source maps
issue-detail-breadcrumbs = Rastros
issue-detail-th-time = Hora
issue-detail-th-category = Categoría
issue-detail-th-message = Mensaje
issue-detail-crumb-data = datos
issue-detail-tags = Etiquetas
issue-detail-contexts = Contextos
issue-detail-request = Solicitud
issue-detail-headers = Encabezados
issue-detail-th-header = Encabezado
issue-detail-query-string = Cadena de consulta
issue-detail-body = Cuerpo
issue-detail-environment = Entorno
issue-detail-user-reports = Informes de usuario
issue-detail-anonymous = Anónimo
issue-detail-attachments = Adjuntos
issue-detail-att-filename = Nombre de archivo
issue-detail-att-type = Tipo
issue-detail-att-size = Tamaño
issue-detail-download = Descargar
issue-detail-raw-json = JSON sin procesar
issue-detail-no-events = No se encontraron eventos para este problema.
issue-detail-ev-id = ID del evento
issue-detail-ev-timestamp = Marca de tiempo
issue-detail-ev-platform = Plataforma
issue-detail-events-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventos
}
issue-detail-props-heading = Propiedades del problema
issue-detail-fingerprint = Fingerprint
issue-detail-tag-facets = Facetas de etiquetas
issue-detail-discard-undo-title = Reanudar la aceptación de eventos futuros con este fingerprint
issue-detail-discard-undo = Deshacer descarte
issue-detail-discard-confirm = ¿Descartar todos los eventos futuros con este fingerprint?
issue-detail-discard-title = Descartar silenciosamente los eventos futuros que coincidan con este fingerprint
issue-detail-discard = Descartar eventos futuros
