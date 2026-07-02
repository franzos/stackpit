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
releases-list-period-24h = Últimas 24 h
releases-list-period-7d = Últimos 7 días
releases-list-period-30d = Últimos 30 días
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
release-health-heading = Estado de release
release-health-sessions-heading = Sesiones a lo largo del tiempo
release-health-empty = No hay datos de sesión disponibles. Los eventos de sesión con un campo <code class="text-mono">status</code> aparecerán aquí.
release-health-col-release = Release
release-health-col-sessions = Sesiones
release-health-col-ok = OK
release-health-col-crashed = Con fallos
release-health-col-errored = Con errores
release-health-col-crash-free-sessions = Sesiones sin fallos
release-health-col-crash-free-users = Usuarios sin fallos
release-health-na = n/d
