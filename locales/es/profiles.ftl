# Superficie de perfiles: la lista de perfiles por proyecto y la página de
# detalle del perfil. Reutiliza nav-profiles. Los strings contados usan plurales
# tv_count ([one]/[other]).

# --- Sufijo de título ---
profiles-title-suffix = — Stackpit

# --- Lista de perfiles ---
profiles-list-empty = No se encontraron perfiles. Los eventos de perfil con <code class="text-mono">item_type = "profile"</code> aparecerán aquí.
profiles-col-event-id = ID del evento
profiles-col-transaction = Transacción
profiles-col-platform = Plataforma
profiles-col-release = Release
profiles-col-environment = Entorno
profiles-col-timestamp = Marca de tiempo

# --- Detalle del perfil ---
profiles-detail-heading = Perfil
profiles-detail-raw-payload = Payload sin procesar

# --- Paginación ---
profiles-pagination-label = Paginación
profiles-pagination-prev = « Anterior
profiles-pagination-next = Siguiente »
profiles-count = { $count ->
    [one] { $count } perfil
   *[other] { $count } perfiles
}
