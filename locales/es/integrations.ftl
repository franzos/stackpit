# Superficie de integraciones: la lista (templates/integrations.html) y los tres
# formularios de "añadir" (webhook, slack, correo electrónico). Reutiliza
# nav-settings/nav-integrations. Los espacios separadores viven en el template.
# integrations-empty contiene markup <strong> en línea y el glifo de flecha, y se
# renderiza con |safe.
integrations-page-title = Integraciones — Stackpit
integrations-subtitle = Salidas de webhook, Slack y correo electrónico. El enrutamiento por proyecto se define en la configuración de cada proyecto.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + Correo electrónico
integrations-license-required-badge = Requiere licencia
integrations-empty = Aún no hay integraciones. Añade una arriba para empezar a recibir notificaciones. Después de añadirla, actívala por proyecto en <strong>Configuración del proyecto → Integraciones</strong>.
integrations-col-name = Nombre
integrations-col-type = Tipo
integrations-col-endpoint = Endpoint
integrations-col-created = Creada
integrations-delete-confirm = ¿Eliminar esta integración? Se quitará de todos los proyectos.
integrations-test = Probar
integrations-delete = Eliminar
flash-test-failed = Prueba fallida: { $error }

# Etiquetas/botones de formulario compartidos entre los tres formularios.
integrations-cancel = Cancelar
integrations-optional = (opcional)
integrations-required = (obligatorio)
integrations-create = Crear integración

# --- Añadir webhook ---
integrations-webhook-title = Añadir webhook — Stackpit
integrations-webhook-breadcrumb = Añadir webhook
integrations-webhook-heading = Añadir integración de webhook
integrations-webhook-name-placeholder = p. ej. Alertas de producción
integrations-webhook-url-label = URL del webhook
integrations-webhook-secret-label = Secreto HMAC
integrations-webhook-secret-placeholder = Secreto de firma opcional

# --- Añadir Slack ---
integrations-slack-title = Añadir Slack — Stackpit
integrations-slack-breadcrumb = Añadir Slack
integrations-slack-heading = Añadir integración de Slack
integrations-slack-name-placeholder = p. ej. canal #alerts
integrations-slack-url-label = URL del webhook de Slack

# --- Añadir correo electrónico ---
integrations-email-title = Añadir correo electrónico — Stackpit
integrations-email-breadcrumb = Añadir correo electrónico
integrations-email-heading = Añadir integración de correo electrónico
integrations-email-name-placeholder = p. ej. Alertas por correo del equipo
integrations-email-lock-pre = El proveedor y el remitente provienen de la
integrations-email-lock-post = configuración del servidor; esta integración solo elige el destinatario.
integrations-email-provider-label = Proveedor
integrations-email-token-label = Token de API
integrations-email-token-placeholder-default = Déjalo vacío para usar el predeterminado
integrations-email-token-placeholder = Token de API del proveedor
integrations-email-from-label = Dirección del remitente
integrations-email-fromname-label = Nombre del remitente
integrations-email-smtp-hint = SMTP usa la conexión [email] del servidor; no se necesita un token por integración.

# Gestor de incidencias
integrations-add-tracker = + Gestor de incidencias
integrations-tracker-title = Añadir gestor de incidencias — Stackpit
integrations-tracker-breadcrumb = Añadir gestor de incidencias
integrations-tracker-heading = Añadir integración de gestor de incidencias
integrations-tracker-kind-label = Gestor
integrations-tracker-name-placeholder = p. ej. GitHub Issues
integrations-tracker-url-label = URL base
integrations-tracker-token-label = Token de API
integrations-tracker-token-placeholder = Token de acceso personal
integrations-tracker-target-help = El repositorio de destino sale de los ajustes de repositorio de cada proyecto, así que no se configura aquí. Añade el repositorio en los ajustes del proyecto.
integrations-global-label = Entregar a todos los proyectos
integrations-global-help = Las alertas van a todos los proyectos de esta organización, salvo los que excluyas en la página de esta integración. Los filtros de nivel y entorno por proyecto se siguen aplicando encima.
integrations-global-badge = toda la organización
integrations-global-save = Guardar enrutado
integrations-global-on = Entregar a toda la organización
integrations-global-off = Dejar de entregar a toda la organización

# Detalle de la integración: enrutado por proyecto
integrations-detail-title = Integración — Stackpit
integrations-back = Volver a integraciones
integrations-projects-heading = Enrutado por proyecto
integrations-projects-hint-global = Esta integración entrega a todos los proyectos de abajo salvo que la excluyas. Excluir es la única forma de salir; no hay lista de inclusión.
integrations-projects-hint-per-project = Esta integración solo entrega donde un proyecto la ha activado. Márcala para toda la organización si quieres que entregue en todas partes.
integrations-projects-hint-tracker = Los gestores de incidencias se emparejan con los repositorios de un proyecto por forja y por host. Excluir un proyecto deja este gestor fuera de sus opciones de creación.
integrations-projects-empty = Esta organización todavía no tiene proyectos.
integrations-col-project = Proyecto
integrations-col-state = Estado
integrations-project-archived = archivado
integrations-state-default = Entregando
integrations-state-customised = Personalizado
integrations-state-excluded = Excluido
integrations-state-no-repo = Sin repositorio coincidente
integrations-state-not-routed = No activado
integrations-exclude = Excluir
integrations-include = Incluir
integrations-email-to-label = Destinatario por defecto
integrations-email-to-help = Se usa donde un proyecto no ha puesto su propia dirección. Obligatorio para una integración de toda la organización.
integrations-summary-delivering = { $count ->
    [one] { $count } entrega
   *[other] { $count } entregan
}
integrations-summary-excluded = { $count ->
    [one] { $count } excluido
   *[other] { $count } excluidos
}
integrations-summary-inert = { $count ->
    [one] { $count } sin entrega
   *[other] { $count } sin entrega
}
integrations-search-placeholder = Filtrar por nombre de proyecto
integrations-search-label = Filtrar proyectos
integrations-search-submit = Filtrar
integrations-sort-label = Ordenar proyectos
integrations-sort-state = Los que entregan primero
integrations-sort-name = Por nombre
integrations-pagination-label = Páginas de entrega por proyecto
integrations-projects-count = { $count ->
    [one] { $count } proyecto
   *[other] { $count } proyectos
}
