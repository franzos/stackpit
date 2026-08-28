# Superficie de problemas: la lista de problemas agrupados por fingerprint y la
# página de detalle. issue-detail-exception-stacktrace contiene un &amp; en línea
# y se renderiza con |safe. Los strings contados usan plurales tv_count ([one]/[other]).

# --- Labels compartidos (lista de problemas + detalle) ---
issues-label-title = Título
issues-label-level = Nivel
issues-label-events = Eventos
issues-label-users = Usuarios
issues-label-trend = Tendencia
issues-trend-tooltip = Volumen de eventos en el periodo seleccionado
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
issues-list-filter-environment = Filtrar por entorno
issues-list-environment-all = Todos los entornos
issues-period-label = Rango de tiempo
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
issue-detail-in-app-only = Solo frames de la aplicación
issue-detail-reverse-order = Invertir el orden
issue-detail-copy = Copiar
issue-detail-copy-frame = Copiar este frame
issue-detail-library-frames = { $count ->
    [one] { $count } frame de biblioteca
   *[other] { $count } frames de biblioteca
}
issue-detail-minified-hint = Estos frames parecen minificados y no se aplicó ningún source map.
issue-detail-minified-hint-link = Subir source maps
issue-detail-breadcrumbs = Rastros
issue-detail-th-time = Hora
issue-detail-th-category = Categoría
issue-detail-th-message = Mensaje
issue-detail-crumb-data = datos
issue-detail-crumb-filter = Filtrar breadcrumbs por tipo
issue-detail-crumb-filter-all = Todos los tipos
issue-detail-tags = Etiquetas
issue-detail-contexts = Contextos
issue-detail-additional-data = Datos adicionales
issue-detail-view-replay = Ver replay
issue-detail-view-trace = Ver traza
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
issue-detail-create-external-issue = Crear incidencia
issue-detail-external-tracker = Gestor externo
issue-detail-view-on = Ver en
flash-tracker-create-failed = No se pudo crear la incidencia. Revisa el token y el repositorio de la integración e inténtalo de nuevo.
flash-tracker-config-incomplete = A esta integración le falta un repositorio o un token. Corrígelo en los ajustes de la integración.
issue-detail-external-unlink = Desvincular
issue-detail-external-unlink-confirm = ¿Quitar este enlace? La incidencia sigue en la forja: ciérrala o bórrala allí.
issue-detail-external-orphaned = integración eliminada
flash-tracker-unlinked = Enlace eliminado. La incidencia sigue existiendo en la forja.
flash-tracker-ambiguous = Este proyecto tiene más de un repositorio en el que este gestor puede crear la incidencia. Elige uno e inténtalo de nuevo.
issue-detail-crumbs-truncated = { $count ->
    [one] Muestra el rastro más reciente.
   *[other] Muestra los { $count } más recientes.
}
issue-detail-crumbs-show-all = { $count ->
    [one] Mostrar el único rastro
   *[other] Mostrar los { $count }
}
issue-detail-external-state-open = abierto
issue-detail-external-state-closed = cerrado
