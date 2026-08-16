# Página de alertas y resúmenes (templates/alerts.html). Reutiliza nav-settings
# y nav-alerts-digests. Los espacios separadores viven en el template, por lo que
# los valores no llevan espacios al inicio/final. alerts-page-title conserva la
# entidad &amp; y se renderiza con |safe.
alerts-page-title = Alertas &amp; resúmenes — Stackpit
alerts-notify-help-pre = Las notificaciones se envían a través de las integraciones en la página
alerts-notify-help-post = .

# --- Tipos de notificación ---
alerts-notify-types-heading = Tipos de notificación
alerts-notify-types-desc = Las alertas de nuevo problema y regresión se disparan con cada problema recién visto o reaparecido. Las reglas de umbral se disparan por volumen de eventos en una ventana; los resúmenes son síntesis periódicas. Esta lista solo cubre las integraciones que un proyecto ha vinculado por sí mismo — una integración de toda la organización llega a cada proyecto y se gestiona en la página de integraciones.
alerts-notify-types-empty = Ningún proyecto ha vinculado una integración propia. Las integraciones de toda la organización no aparecen aquí y pueden estar entregando igualmente; consulta la página de integraciones.
alerts-col-integration = Integración
alerts-col-new-issues = Nuevos problemas
alerts-col-regressions = Regresiones
alerts-col-digests = Resúmenes
alerts-notify-save = Guardar

# --- Reglas de umbral ---
alerts-threshold-heading = Reglas de umbral
alerts-threshold-desc = Se activa cuando un problema recibe más de N eventos en una ventana de tiempo.
alerts-rules-empty = Aún no hay reglas de alerta.
alerts-col-scope = Ámbito
alerts-col-issue = Problema
alerts-col-threshold = Umbral
alerts-col-window = Ventana
alerts-col-cooldown = Tiempo de espera
alerts-scope-global = Global
alerts-fingerprint-any = Cualquiera
alerts-rule-delete-confirm = ¿Eliminar esta regla de alerta?
alerts-delete-label = Eliminar
alerts-add-rule = + Añadir regla de alerta
alerts-all-projects = Todos los proyectos
alerts-project-fallback = Proyecto { $id }
alerts-fingerprint-label = Fingerprint del problema
alerts-fingerprint-hint = (vacío = cualquiera)
alerts-fingerprint-placeholder = cualquier problema
alerts-fingerprint-help = Un fingerprint identifica un problema (eventos agrupados). Visible en la URL de cualquier página de problema. Déjalo vacío para coincidir con todos los problemas del ámbito.
alerts-unit-s = (s)
alerts-create-rule = Crear regla

# --- Programaciones de resúmenes ---
alerts-digest-heading = Programaciones de resúmenes
alerts-digest-desc = Resúmenes periódicos de actividad — informes diarios o semanales en lugar de ruido por evento.
alerts-digests-empty = Aún no hay programaciones de resúmenes.
alerts-col-interval = Intervalo
alerts-col-last-sent = Último envío
alerts-col-enabled = Activado
alerts-never = Nunca
alerts-yes = Sí
alerts-no = No
alerts-digest-delete-confirm = ¿Eliminar esta programación de resúmenes?
alerts-add-digest = + Añadir programación de resúmenes
alerts-interval-daily = Diario (24h)
alerts-interval-weekly = Semanal (7d)
alerts-interval-hourly = Cada hora
alerts-create-schedule = Crear programación
