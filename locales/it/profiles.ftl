# Superficie profili: l'elenco profili per progetto e la pagina di dettaglio.
# Usa nav-profiles. Le stringhe conteggiate usano i plurali tv_count ([one]/[other]).

# --- Suffisso del titolo ---
profiles-title-suffix = — Stackpit

# --- Elenco profili ---
profiles-list-empty = Nessun profilo trovato. Gli eventi di profilo con <code class="text-mono">item_type = "profile"</code> appariranno qui.
profiles-col-event-id = ID evento
profiles-col-transaction = Transazione
profiles-col-platform = Piattaforma
profiles-col-release = Release
profiles-col-environment = Ambiente
profiles-col-timestamp = Timestamp

# --- Dettaglio profilo ---
profiles-detail-heading = Profilo
profiles-detail-raw-payload = Payload grezzo

# --- Impaginazione ---
profiles-pagination-label = Impaginazione
profiles-pagination-prev = « Precedente
profiles-pagination-next = Successivo »
profiles-count = { $count ->
    [one] { $count } profilo
   *[other] { $count } profili
}
