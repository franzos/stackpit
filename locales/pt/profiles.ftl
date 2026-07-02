# Interface de perfis: a lista de perfis por projeto e a página de detalhe do
# perfil. Reutiliza nav-profiles. As strings com contagem usam plurais tv_count
# ([one]/[other]).

# --- Sufixo do título da página ---
profiles-title-suffix = — Stackpit

# --- Lista de perfis ---
profiles-list-empty = Nenhum perfil encontrado. Os eventos de perfil com <code class="text-mono">item_type = "profile"</code> aparecerão aqui.
profiles-col-event-id = ID do evento
profiles-col-transaction = Transação
profiles-col-platform = Plataforma
profiles-col-release = Release
profiles-col-environment = Ambiente
profiles-col-timestamp = Data/hora

# --- Detalhe do perfil ---
profiles-detail-heading = Perfil
profiles-detail-raw-payload = Dados em bruto

# --- Paginação ---
profiles-pagination-label = Paginação
profiles-pagination-prev = « Anterior
profiles-pagination-next = Seguinte »
profiles-count = { $count ->
    [one] { $count } perfil
   *[other] { $count } perfis
}
