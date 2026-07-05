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
integrations-email-smtp-hint = SMTP usa la conexión [email.smtp] del servidor; no se necesita un token por integración.
