# Superficie replay: l'elenco replay per progetto e la pagina di dettaglio.
# Usa nav-replays. Le stringhe conteggiate usano i plurali tv_count ([one]/[other]).

# --- Suffisso del titolo ---
replays-title-suffix = — Stackpit

# --- Elenco replay ---
replays-list-empty = Nessun replay trovato. Gli eventi di replay appariranno qui.
replays-col-event-id = ID evento
replays-col-type = Tipo
replays-col-release = Release
replays-col-environment = Ambiente
replays-col-timestamp = Timestamp

# --- Dettaglio replay ---
replays-detail-heading = Replay
replays-detail-note = La riproduzione della registrazione non è ancora disponibile. I dati grezzi del replay sono mostrati qui sotto.
replays-detail-raw-payload = Payload grezzo

# --- Impaginazione ---
replays-pagination-label = Impaginazione
replays-pagination-prev = « Precedente
replays-pagination-next = Successivo »
replays-count = { $count ->
    [one] { $count } replay
   *[other] { $count } replay
}
