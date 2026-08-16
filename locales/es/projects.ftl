# Superficie de proyectos: lista, nuevo, configuración (general/keys/sourcemaps/
# filtros), integraciones y la confirmación de creación. Los valores con |safe
# contienen markup HTML en línea; las etiquetas permanecen idénticas, solo el
# texto está traducido.

# --- Lista de proyectos ---
projects-list-title = Proyectos — Stackpit
projects-list-heading = Proyectos
projects-list-subtitle = Supervisa el estado de toda tu arquitectura.
projects-list-all-events = Todos los eventos
projects-list-all-releases = Todos los releases
projects-list-new = + Nuevo proyecto
projects-list-search-placeholder = Busca proyectos por nombre, plataforma o propietario…
projects-list-search-label = Buscar proyectos
projects-list-filter = Filtrar
projects-org-filter-label = Filtrar por organización
projects-org-filter-all = Todas las organizaciones
projects-list-empty = No se encontraron proyectos. Los eventos aparecerán aquí una vez ingeridos.
projects-period-label = Rango de tiempo
projects-col-project = Proyecto
projects-col-platforms = Plataformas
projects-col-issues = Problemas
projects-col-events = Eventos
projects-col-breakdown = Desglose
projects-col-release = Release
projects-col-first-seen = Visto por primera vez
projects-col-last-seen = Visto por última vez
projects-breakdown-errors = Errores:
projects-breakdown-transactions = Transacciones:
projects-breakdown-sessions = Sesiones:
projects-breakdown-other = Otros:
projects-legend-errors = Errores
projects-legend-transactions = Transacciones
projects-legend-sessions = Sesiones
projects-legend-other = Otros

# --- Compartido en los formularios de proyecto ---
projects-optional = (opcional)
projects-cancel = Cancelar
projects-remove = Quitar
projects-delete = Eliminar
projects-name-placeholder = Mi proyecto

# --- Nuevo proyecto ---
projects-new-title = Nuevo proyecto — Stackpit
projects-new-heading = Nuevo proyecto
projects-new-name-label = Nombre del proyecto
projects-new-platform-label = Plataforma
projects-new-platform-select = Selecciona una plataforma…
projects-new-platform-other = Otra
projects-new-platform-native = Native (C/C++)
projects-new-submit = Crear proyecto

# --- Pestañas de configuración (compartidas por las páginas de ajustes) ---
projects-tab-general = General
projects-tab-sdk = Configuración del SDK
projects-tab-sourcemaps = Source maps
projects-tab-filters = Filtros
projects-tab-integrations = Integraciones

# --- Configuración: general ---
projects-settings-heading = Configuración
projects-settings-archived = (archivado)
projects-settings-name-heading = Nombre del proyecto
projects-settings-display-name = Nombre mostrado
projects-settings-save-name = Guardar nombre
projects-settings-info-heading = Información del proyecto
projects-settings-status = Estado
projects-settings-source = Origen
projects-repos-heading = Repositorios de código fuente
projects-repos-help = Vincula los stack frames al código fuente en tu forge. Registra un release con un SHA de commit mediante <code class="text-mono">sentry-cli</code> para activar los enlaces.
projects-repos-empty = No hay repositorios configurados.
projects-repos-url-label = URL del repositorio
projects-repos-col-forge = Forge
projects-repos-template = Plantilla de URL
projects-repos-auto = automático
projects-repos-remove-confirm = ¿Quitar este repositorio?
projects-repos-add = Añadir repositorio
projects-repos-add-help = Añade enlaces de código fuente clicables (p. ej. "Ver en GitHub") junto a los stack frames. Requiere un release con un SHA de commit — el tipo de forge se detecta automáticamente. Compatibles: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. Para otras forges, proporciona una plantilla de URL.
projects-danger-heading = Zona de peligro
projects-archive-desc = Archiva este proyecto. Los proyectos archivados rechazan nuevos eventos.
projects-archive-confirm = ¿Archivar este proyecto? Se rechazarán los nuevos eventos.
projects-archive-submit = Archivar proyecto
projects-unarchive-desc = Desarchiva este proyecto para volver a aceptar eventos.
projects-unarchive-submit = Desarchivar proyecto
projects-delete-desc = Elimina permanentemente este proyecto y todos sus datos. Esto no se puede deshacer.
projects-delete-confirm = ¿Eliminar este proyecto y TODOS sus datos? Esto no se puede deshacer.
projects-delete-submit = Eliminar proyecto
projects-move-heading = Mover a otra organización
projects-move-desc = Mueve este proyecto a otra organización de la que seas propietario. Sus datos y DSN siguen siendo válidos, pero las integraciones de notificación se desvinculan y deben volver a añadirse en la nueva organización.
projects-move-target-label = Organización de destino
projects-move-confirm-pre = Escribe
projects-move-confirm-post = para confirmar.
projects-move-confirm-placeholder = Nombre del proyecto
projects-move-confirm-dialog = ¿Mover este proyecto a la organización seleccionada?
projects-move-submit = Mover proyecto
projects-move-err-invalid-target = Organización de destino no válida.
projects-move-err-name-mismatch = El nombre del proyecto no coincide.
projects-move-err-denied = No eres propietario de la organización de destino.
projects-move-err-conflict = No se pudo mover el proyecto; es posible que haya cambiado. Inténtalo de nuevo.

# --- Configuración: SDK / keys ---
projects-keys-title = Configuración del SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = No hay keys registrados. Crea un key abajo para obtener un DSN.
projects-keys-list-heading = Keys del proyecto
projects-keys-empty = No hay keys registrados para este proyecto.
projects-keys-col-public = Clave pública
projects-keys-col-label = Etiqueta
projects-keys-col-status = Estado
projects-keys-col-created = Creado
projects-keys-delete-confirm = ¿Eliminar este key? Los SDK que lo usen dejarán de funcionar.
projects-keys-create-heading = Crear key
projects-keys-label-label = Etiqueta
projects-keys-label-placeholder = p. ej. production, staging
projects-keys-create-submit = Crear key

# --- Configuración: source maps ---
projects-sourcemaps-title = Source Maps
projects-sourcemaps-apikey-heading = Clave de API
projects-sourcemaps-apikey-desc = La subida de source maps requiere una clave de API. Específica de este proyecto y utilizable solo para operaciones de source maps.
projects-sourcemaps-key-generated = Clave generada:
projects-sourcemaps-key-warning = Copia esta clave ahora — no se volverá a mostrar.
projects-sourcemaps-col-key = Clave
projects-sourcemaps-regen-confirm = ¿Regenerar la clave? La clave actual dejará de funcionar.
projects-sourcemaps-regen = Regenerar
projects-sourcemaps-empty = No hay clave de API de source maps para este proyecto.
projects-sourcemaps-generate = Generar clave
projects-sourcemaps-setup-heading = Configuración
projects-sourcemaps-setup-desc = Usa <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> para subir source maps. Define estas variables de entorno:
projects-sourcemaps-then-upload = Después sube:

# --- Configuración: filtros ---
projects-filters-inbound-heading = Filtros de entrada
projects-filters-inbound-desc = Filtros integrados que descartan eventos que coinciden con patrones de ruido comunes.
projects-filters-browser-ext = Extensiones del navegador — descartar eventos de extensiones de Chrome/Firefox/Safari
projects-filters-localhost = Localhost — descartar eventos de localhost, 127.0.0.1, IPs privadas
projects-filters-inbound-submit = Guardar filtros de entrada
projects-filters-message-heading = Filtros de mensaje
projects-filters-message-help = Patrones glob comparados con los títulos de los eventos. Usa <code class="text-mono">*</code> para cualquier secuencia, <code class="text-mono">?</code> para un solo carácter.
projects-filters-col-pattern = Patrón
projects-filters-message-empty = No hay filtros de mensaje configurados.
projects-filters-add-pattern = Añadir patrón
projects-filters-message-submit = Añadir filtro de mensaje
projects-filters-ratelimit-heading = Límite de tasa
projects-filters-ratelimit-desc = Máximo de eventos por minuto para este proyecto. 0 = ilimitado.
projects-filters-ratelimit-label = Eventos por minuto
projects-filters-ratelimit-submit = Guardar límite de tasa
projects-filters-env-heading = Entornos excluidos
projects-filters-env-desc = Los eventos de estos entornos se descartarán silenciosamente.
projects-filters-col-environment = Entorno
projects-filters-env-empty = No hay entornos excluidos.
projects-filters-env-add-label = Añadir entorno excluido
projects-filters-env-submit = Excluir entorno
projects-filters-release-heading = Filtros de release
projects-filters-release-desc = Patrones glob comparados con las versiones de release. Los eventos coincidentes se descartan.
projects-filters-release-empty = No hay filtros de release.
projects-filters-release-submit = Añadir filtro de release
projects-filters-ua-heading = Filtros de user-agent
projects-filters-ua-desc = Patrones glob comparados con los encabezados User-Agent. Los patrones integrados para kube-probe y comprobadores de estado siempre están activos.
projects-filters-ua-empty = No hay filtros de user-agent personalizados.
projects-filters-ua-submit = Añadir filtro de user-agent
projects-filters-rules-heading = Reglas personalizadas
projects-filters-rules-desc = Reglas avanzadas que comparan campos de eventos. Las reglas de mayor prioridad se evalúan primero.
projects-filters-col-field = Campo
projects-filters-col-operator = Operador
projects-filters-col-value = Valor
projects-filters-col-action = Acción
projects-filters-col-priority = Prioridad
projects-filters-rules-empty = No hay reglas personalizadas.
projects-filters-sample-rate-label = Tasa de muestreo
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Añadir regla
projects-filters-op = { $op ->
    [not_equals] no es igual a
    [contains] contiene
    [not_contains] no contiene
    [starts_with] empieza por
    [in] en
    [not_in] no en
   *[equals] es igual a
}
projects-filters-action = { $action ->
    [sample] muestrear
   *[drop] descartar
}
projects-filters-ip-heading = Lista de bloqueo de IP
projects-filters-ip-desc = Bloques CIDR o IPs individuales. Los eventos de las IPs bloqueadas se descartan silenciosamente.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = No hay bloqueos de IP configurados.
projects-filters-ip-add-label = Añadir CIDR
projects-filters-ip-submit = Bloquear rango de IP
projects-filters-discard-heading = Estadísticas de descarte
projects-filters-discard-window = (últimos 7 días)
projects-filters-col-date = Fecha
projects-filters-col-reason = Motivo
projects-filters-col-count = Recuento

# Etiquetas de entidad de filtro, interpoladas en flash-not-found-filter al eliminar.
projects-filter-label-message = filtro de mensaje
projects-filter-label-environment = filtro de entorno
projects-filter-label-release = filtro de release
projects-filter-label-user-agent = filtro de user-agent
projects-filter-label-rule = regla de filtro

# --- Configuración: integraciones ---
projects-integrations-active-heading = Integraciones activas
projects-integrations-active-empty = No hay integraciones activadas. Añade primero una integración global en la página de <a class="text-primary" href="/web/settings/integrations/">Integraciones</a> y luego actívala aquí. Puedes acotar cada una por nivel mínimo y entorno para que el ruido de dev no llegue a los canales de prod.
projects-integrations-deactivate-confirm = ¿Desactivar esta integración para el proyecto?
projects-integrations-deactivate = Desactivar
projects-integrations-notify-new-issues = Nuevos problemas
projects-integrations-notify-regressions = Regresiones
projects-integrations-notify-threshold = Alertas de umbral
projects-integrations-notify-digests = Resúmenes
projects-integrations-min-level = Nivel mínimo
projects-integrations-level-any = Cualquiera
projects-integrations-env-filter = Filtro de entorno
projects-integrations-env-placeholder = p. ej. production
projects-integrations-to-address = Dirección de destino
projects-integrations-to-address-note = (solo integraciones de correo electrónico)
projects-integrations-activate-heading = Activar integración
projects-integrations-integration-label = Integración
projects-integrations-activate-submit = Activar
projects-integrations-available-empty = No hay integraciones disponibles. <a class="text-primary" href="/web/settings/integrations/">Crea una primero</a>.

# --- Proyecto creado ---
projects-created-word = creado
projects-created-breadcrumb = Creado
projects-created-heading = Proyecto creado
projects-created-subtitle = Usa el DSN de abajo para configurar tu SDK.
projects-created-settings-btn = Configuración del proyecto
projects-created-back = Volver a los proyectos
projects-created-details-heading = Detalles del proyecto
projects-created-col-id = ID del proyecto
projects-created-sdk-desc-before = Instala el SDK de Sentry para
projects-created-sdk-desc-after = e inicialízalo con el DSN de arriba.
projects-created-docs-javascript = Docs de Sentry JavaScript →
projects-created-docs-python = Docs de Sentry Python →
projects-created-docs-rust = Docs de Sentry Rust →
projects-created-docs-go = Docs de Sentry Go →
projects-created-docs-node = Docs de Sentry Node.js →
projects-created-docs-java = Docs de Sentry Java →
projects-created-docs-ruby = Docs de Sentry Ruby →
projects-created-docs-php = Docs de Sentry PHP →
projects-created-docs-elixir = Docs de Sentry Elixir →
projects-created-docs-dotnet = Docs de Sentry .NET →
projects-created-docs-apple = Docs de Sentry Apple →
projects-created-docs-kotlin = Docs de Sentry Kotlin →
projects-created-docs-native = Docs de Sentry Native →
projects-created-docs-generic = Docs de plataforma de Sentry →
projects-repos-forge-override = Tipo de forja
projects-repos-forge-detected = Detectado automáticamente
projects-repos-forge-override-help = Solo hay que ponerlo si el tipo detectado es incorrecto, normalmente en una instancia autoalojada cuyo nombre de host no dice nada sobre la forja que ejecuta.
projects-repos-prefix = Prefijo de ruta
projects-repos-prefix-placeholder = services/api/
projects-repos-prefix-help = Qué marcos de la pila pertenecen a este repositorio, comparados con el principio del nombre de archivo del marco. Déjalo vacío en un proyecto de un solo repositorio. En cuanto un repositorio de aquí tenga prefijo, solo se aplica la coincidencia por prefijo y los repositorios sin prefijo dejan de producir enlaces al código.
projects-repos-col-prefix = Prefijo de ruta
projects-integrations-reset = Restablecer los valores de la organización
projects-integrations-reset-confirm = ¿Descartar los ajustes de este proyecto y volver a entregar con los valores de la organización?
projects-integrations-global-hint = Esta integración entrega a todos los proyectos de la organización. Los ajustes de abajo solo la personalizan aquí; para detener la entrega por completo, excluye este proyecto en la página de la integración.
projects-integrations-tracker-hint = El repositorio de destino sale de los ajustes de repositorio de este proyecto, no de aquí.
