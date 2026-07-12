# Superficie de eventos: la lista de eventos entre proyectos y la página de
# detalle del evento. event-detail-exception-stacktrace contiene un &amp; en línea
# y se renderiza con |safe. Los strings contados usan plurales tv_count ([one]/[other]).

# --- Labels compartidos (lista de eventos + detalle) ---
events-label-title = Título
events-label-type = Tipo
events-label-level = Nivel
events-label-platform = Plataforma
events-label-environment = Entorno
events-label-time = Hora
events-label-value = Valor

# --- Paginación (compartida) ---
events-pagination-label = Paginación
events-pagination-prev = « Anterior
events-pagination-next = Siguiente »

# --- Sufijo de título (títulos con prefijo dinámico) ---
events-title-suffix = — Stackpit

# --- Lista de eventos ---
events-list-title = Eventos — Stackpit
events-heading = Eventos
events-list-search-placeholder = Buscar eventos…
events-list-search-label = Buscar eventos
events-list-select = Seleccionar evento
events-list-filter-level = Filtrar por nivel
events-list-level-all = Todos los niveles
events-list-filter-type = Filtrar por tipo
events-list-type-all = Todos los tipos
events-list-project-placeholder = ID de proyecto
events-list-filter-project = Filtrar por proyecto
events-list-filter-submit = Filtrar
events-list-empty = Ningún evento coincide con los filtros actuales.
events-untitled = (sin título)
events-col-project = Proyecto

# --- Acciones masivas ---
events-bulk-delete = Eliminar
events-bulk-delete-selected-confirm = ¿Eliminar los eventos seleccionados?
events-bulk-delete-all = Eliminar los { $count } coincidentes
events-bulk-delete-all-confirm = { $count ->
    [one] ¿Eliminar permanentemente el { $count } evento coincidente?
   *[other] ¿Eliminar permanentemente los { $count } eventos coincidentes?
}

# --- Recuento (paginación) ---
events-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventos
}

# --- Detalle del evento ---
event-detail-event = Evento
event-detail-event-id-label = event_id:
event-detail-nav-label = Navegación de eventos
event-detail-nav-newer = « Más reciente
event-detail-nav-older = Más antiguo »
event-detail-nav-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventos
}
event-detail-nav-in-issue = en el problema
event-detail-user-feedback = Comentarios del usuario
event-detail-anonymous = Anónimo
event-detail-related-event = Evento relacionado:
event-detail-exception-stacktrace = Excepción &amp; Stacktrace
event-detail-handled = gestionado
event-detail-unhandled = no gestionado
event-detail-in = en
event-detail-var-name = Variable
event-detail-no-source = No hay contexto de código fuente disponible
event-detail-breadcrumbs = Rastros
event-detail-th-category = Categoría
event-detail-th-message = Mensaje
event-detail-tags = Etiquetas
event-detail-contexts = Contextos
event-detail-request = Solicitud
event-detail-headers = Encabezados
event-detail-th-header = Encabezado
event-detail-query-string = Cadena de consulta
event-detail-body = Cuerpo
event-detail-user-reports = Informes de usuario
event-detail-attachments = Adjuntos
event-detail-att-filename = Nombre de archivo
event-detail-att-size = Tamaño
event-detail-download = Descargar
event-detail-web-vitals = Web Vitals
event-detail-raw-json = JSON sin procesar
event-detail-props-heading = Propiedades del evento
event-detail-prop-event-id = ID del evento
event-detail-prop-timestamp = Marca de tiempo
event-detail-prop-transaction = Transacción
event-detail-prop-release = Release
event-detail-prop-server = Servidor
event-detail-prop-sdk = SDK
event-detail-prop-received = Recibido
event-detail-user-heading = Usuario
event-detail-user-id = ID
event-detail-user-email = Correo electrónico
event-detail-user-username = Nombre de usuario
event-detail-user-ip = Dirección IP

# --- Informes de cliente (eventos descartados) ---
# Reutiliza events-untitled y events-pagination-* (compartidos, mismo archivo).
client-reports-title = Informes de cliente
client-reports-heading = Informes de cliente
client-reports-dropped-heading = Eventos descartados
client-reports-dropped-subtitle = Lo que los SDK descartaron antes de enviarlo, por categoría y motivo.
client-reports-th-category = Categoría
client-reports-th-reason = Motivo
client-reports-th-reasons = Motivos
client-reports-th-dropped = Descartados
client-reports-empty = No se encontraron informes de cliente para este proyecto.
client-reports-reports-heading = Informes
client-reports-delete = Eliminar
client-reports-delete-selected-confirm = ¿Eliminar los informes seleccionados?
client-reports-th-event-id = ID del evento
client-reports-th-title = Título
client-reports-th-timestamp = Marca de tiempo
client-reports-th-platform = Plataforma
client-reports-th-release = Release
client-reports-select = Seleccionar informe
client-reports-delete-all = Eliminar los { $count }
client-reports-delete-all-confirm = { $count ->
    [one] ¿Eliminar el { $count } informe coincidente?
   *[other] ¿Eliminar los { $count } informes coincidentes?
}
client-reports-count = { $count ->
    [one] { $count } informe
   *[other] { $count } informes
}

# --- Informes de usuario (comentarios del usuario) ---
# Reutiliza events-untitled y events-pagination-* (compartidos, mismo archivo).
user-reports-title = Informes de usuario
user-reports-heading = Informes de usuario
user-reports-empty = No se encontraron informes de usuario para este proyecto.
user-reports-delete = Eliminar
user-reports-delete-selected-confirm = ¿Eliminar los informes seleccionados?
user-reports-th-event-id = ID del evento
user-reports-th-title = Título
user-reports-th-timestamp = Marca de tiempo
user-reports-th-platform = Plataforma
user-reports-th-release = Release
user-reports-select = Seleccionar informe
user-reports-delete-all = Eliminar los { $count }
user-reports-delete-all-confirm = { $count ->
    [one] ¿Eliminar el { $count } informe coincidente?
   *[other] ¿Eliminar los { $count } informes coincidentes?
}
user-reports-count = { $count ->
    [one] { $count } informe
   *[other] { $count } informes
}
