# Superficie de replays: la lista de replays por proyecto y la página de detalle
# del replay. Reutiliza nav-replays. Los strings contados usan plurales tv_count
# ([one]/[other]).

# --- Sufijo de título ---
replays-title-suffix = — Stackpit

# --- Lista de replays ---
replays-list-empty = No se encontraron replays. Los eventos de replay aparecerán aquí.
replays-col-event-id = ID del evento
replays-col-type = Tipo
replays-col-release = Release
replays-col-environment = Entorno
replays-col-timestamp = Marca de tiempo

# --- Detalle del replay ---
replays-detail-heading = Replay
replays-detail-note = La reproducción de la grabación aún no está disponible. Los datos sin procesar del replay se muestran abajo.
replays-detail-raw-payload = Payload sin procesar
replays-related-errors = Errores en este replay
replays-col-level = Nivel
replays-col-title = Título

# --- Paginación ---
replays-pagination-label = Paginación
replays-pagination-prev = « Anterior
replays-pagination-next = Siguiente »
replays-count = { $count ->
    [one] { $count } replay
   *[other] { $count } replays
}
