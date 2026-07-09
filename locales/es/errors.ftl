# Equivalentes en español de locales/en/errors.ftl. El nombre de marca "Stackpit"
# permanece literal en los templates, como en base.html/login.html.
error-page-title = Error - Stackpit
error-heading = Error
error-back-projects = Volver a los proyectos

# Página de confirmación de invitación creada (solo inglés/locale por defecto).
invite-created-page-title = Invitación creada - Stackpit
invite-created-heading = Invitación creada
invite-created-share = Comparte este enlace. Es válido durante { $ttl } y de un solo uso.
invite-created-back-members = Volver a los miembros

# --- Mensajes flash, de éxito y de validación (dependen del locale) ---

# Diagnósticos de "no encontrado". El prefijo "Error:" se antepone en Rust; el
# valor solo lleva la frase de la entidad más el id.
flash-not-found-project = proyecto no encontrado: { $id }
flash-not-found-key = clave de API no encontrada: { $id }
flash-not-found-integration = integración no encontrada: { $id }
flash-not-found-alert-rule = regla de alerta no encontrada: { $id }
flash-not-found-digest-schedule = programación de resúmenes no encontrada: { $id }
flash-not-found-repo = repositorio no encontrado: { $id }
flash-not-found-project-integration = integración de proyecto no encontrada: { $id }
flash-not-found-filter = { $label } no encontrado

# Validación de reglas de filtro
flash-unrecognized-field = Campo no reconocido: { $value }
flash-unrecognized-operator = Operador no reconocido: { $value }
flash-unrecognized-action = Acción no reconocida: { $value }

# Configuración del proyecto
flash-project-name-updated = Nombre del proyecto actualizado
flash-project-name-too-long = El nombre del proyecto supera la longitud máxima de { $max } caracteres
flash-repo-url-required = La URL del repositorio es obligatoria
flash-repo-url-too-long = La URL del repositorio supera la longitud máxima de 2048 caracteres
flash-repo-added = Repositorio añadido
flash-repo-removed = Repositorio eliminado
flash-project-archived = Proyecto archivado
flash-project-unarchived = Proyecto desarchivado
flash-key-created = Clave creada
flash-key-deleted = Clave eliminada

# Alertas y resúmenes
flash-project-not-found-or-denied = Error: proyecto no encontrado o acceso denegado
flash-alert-rule-created = Regla de alerta creada
flash-alert-rule-deleted = Regla de alerta eliminada
flash-digest-schedule-created = Programación de resúmenes creada
flash-digest-schedule-deleted = Programación de resúmenes eliminada

# Integraciones del proyecto
flash-integration-not-found = Integración no encontrada
flash-integration-activated = Integración activada
flash-integration-updated = Integración actualizada
flash-integration-deactivated = Integración desactivada

# Integraciones de la organización
flash-name-required = El nombre es obligatorio
flash-invalid-integration-kind = Tipo de integración no válido
flash-invalid-email-provider = Proveedor de correo electrónico no válido
flash-api-token-required = El token de API es obligatorio.
flash-from-address-required = La dirección del remitente es obligatoria.
flash-smtp-not-configured = SMTP no está configurado. Define [email] host en la configuración del servidor.
flash-invalid-to-address = El destinatario debe ser una dirección de correo electrónico válida.
flash-test-digest-sent = Resumen de prueba en cola para { $count } proyecto(s) hacia sus integraciones con resúmenes activados.
flash-test-digest-sample = Sin actividad reciente, así que se puso en cola un resumen de muestra etiquetado.
flash-test-digest-no-target = Ninguna integración tiene los resúmenes activados para el proyecto de esta programación.
flash-url-required = La URL es obligatoria
flash-secret-not-configured = No se puede guardar el secreto: el cifrado no está configurado. Define STACKPIT_MASTER_KEY para habilitar el almacenamiento de secretos.
flash-integration-created = Integración creada
flash-integration-name-exists = Ya existe una integración con ese nombre.
flash-integration-deleted = Integración eliminada
flash-integration-no-url = La integración no tiene ninguna URL configurada
flash-test-notification-sent = Notificación de prueba enviada

# Filtros de entrada
flash-inbound-filters-updated = Filtros de entrada actualizados
flash-pattern-required = El patrón es obligatorio
flash-message-filter-added = Filtro de mensaje añadido
flash-message-filter-removed = Filtro de mensaje eliminado
flash-rate-limit-updated = Límite de tasa actualizado
flash-environment-required = El entorno es obligatorio
flash-environment-excluded = Entorno excluido
flash-environment-filter-removed = Filtro de entorno eliminado
flash-release-filter-added = Filtro de release añadido
flash-release-filter-removed = Filtro de release eliminado
flash-ua-filter-added = Filtro de user-agent añadido
flash-ua-filter-removed = Filtro de user-agent eliminado
flash-rule-added = Regla añadida
flash-rule-removed = Regla eliminada
flash-cidr-required = El CIDR es obligatorio
flash-invalid-cidr = Formato CIDR no válido
flash-ip-block-added = Bloqueo de IP añadido
flash-ip-block-removed = Bloqueo de IP eliminado

# Nuevo proyecto
flash-project-name-required = El nombre del proyecto es obligatorio
