# Superficie de releases: la lista de releases entre proyectos y la página de
# estado de release por proyecto. Reutiliza nav-releases y nav-health. Los strings
# contados usan plurales tv_count ([one]/[other]).

# --- Sufijo de título ---
releases-title-suffix = — Stackpit

# --- Lista de releases ---
releases-list-search-placeholder = Buscar releases…
releases-list-search-label = Buscar releases
releases-list-project-placeholder = ID de proyecto
releases-list-project-label = Filtrar por proyecto
releases-list-period-label = Periodo de adopción
releases-filter-submit = Filtrar
releases-list-empty = Aún no hay releases. Define un <code class="text-mono">release</code> en tu SDK y aparecerán aquí en cuanto lleguen eventos.
releases-col-version = Versión
releases-col-project = Proyecto
releases-col-issues = Problemas
releases-col-events = Eventos
releases-col-adoption = Adopción
releases-col-first-seen = Visto por primera vez
releases-col-last-seen = Visto por última vez

# --- Paginación ---
releases-pagination-label = Paginación
releases-pagination-prev = « Anterior
releases-pagination-next = Siguiente »
releases-count = { $count ->
    [one] { $count } release
   *[other] { $count } releases
}

# --- Estado de release ---
release-health-title = Estado de release
release-health-sessions-heading = Sesiones a lo largo del tiempo
release-health-period-label = Rango de tiempo
release-health-empty = No hay datos de sesión disponibles. Los eventos de sesión con un campo <code class="text-mono">status</code> aparecerán aquí.
release-health-col-release = Release
release-health-col-sessions = Sesiones
release-health-col-ok = OK
release-health-col-crashed = Con fallos
release-health-col-errored = Con errores
release-health-col-crash-free-sessions = Sesiones sin fallos
release-health-col-error-free-sessions = Sesiones sin errores
release-health-col-crash-free-users = Usuarios sin fallos
release-health-subtitle = Los resultados de sesión son señales de estado informadas por el SDK, no eventos de error. Haz clic en una release para ver sus problemas.
release-health-crashed-title = Ver problemas de esta release
release-health-errored-title = Ver problemas de esta release
release-health-errored-hint = El conteo «con errores» son señales de estado de sesión informadas por el SDK (una sesión que registró un error controlado pero no falló), no eventos de error individuales, y no puede listarse por sesión. Los problemas enlazados son los grupos de errores vistos en esta release.

# --- Detalle de release (por versión) ---
release-detail-sessions-heading = Estado de las sesiones
release-detail-sessions-note = Resultados de sesión informados por el SDK (ok / con errores / con fallos). Son señales de estado, no eventos de error individuales.
release-detail-no-health = No hay datos de sesión para esta release.
release-detail-issues-heading = Problemas en esta release
release-detail-issues-note = Grupos de errores distintos vistos por primera o última vez con esta release.
release-detail-no-issues = No hay problemas registrados para esta release.
release-health-na = n/d
