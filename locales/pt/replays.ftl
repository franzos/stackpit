# Interface de replays: a lista de replays por projeto e a página de detalhe do
# replay. Reutiliza nav-replays. As strings com contagem usam plurais tv_count
# ([one]/[other]).

# --- Sufixo do título da página ---
replays-title-suffix = — Stackpit

# --- Lista de replays ---
replays-list-empty = Nenhum replay encontrado. Os eventos de replay aparecerão aqui.
replays-col-event-id = ID do evento
replays-col-type = Tipo
replays-col-release = Release
replays-col-url = URL
replays-col-user = Utilizador
replays-col-browser = Navegador
replays-col-duration = Duração
replays-col-errors = Erros
replays-col-environment = Ambiente
replays-col-timestamp = Data/hora

# --- Detalhe do replay ---
replays-detail-heading = Replay
replays-detail-note = A reprodução da gravação ainda não está disponível. Os dados de replay em bruto são mostrados abaixo.
replays-detail-raw-payload = Dados em bruto
replays-related-errors = Erros neste replay
replays-col-level = Nível
replays-col-title = Título

# --- Paginação ---
replays-pagination-label = Paginação
replays-pagination-prev = « Anterior
replays-pagination-next = Seguinte »
replays-count = { $count ->
    [one] { $count } replay
   *[other] { $count } replays
}
