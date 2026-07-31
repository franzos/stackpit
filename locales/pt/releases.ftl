# Interface de releases: a lista de releases entre projetos e a página de estado
# do release por projeto. Reutiliza nav-releases e nav-health. As strings com
# contagem usam plurais tv_count ([one]/[other]).

# --- Sufixo do título da página ---
releases-title-suffix = — Stackpit

# --- Lista de releases ---
releases-list-search-placeholder = Pesquisar releases…
releases-list-search-label = Pesquisar releases
releases-list-project-placeholder = ID do projeto
releases-list-project-label = Filtrar por projeto
releases-list-period-label = Período de adoção
releases-list-period-24h = Últimas 24h
releases-list-period-7d = Últimos 7 dias
releases-list-period-30d = Últimos 30 dias
releases-filter-submit = Filtrar
releases-list-empty = Ainda não há releases. Defina um <code class="text-mono">release</code> no seu SDK e aparecerão aqui assim que chegarem eventos.
releases-col-version = Versão
releases-col-project = Projeto
releases-col-issues = Problemas
releases-col-events = Eventos
releases-col-adoption = Adoção
releases-col-first-seen = Visto pela primeira vez
releases-col-last-seen = Visto pela última vez

# --- Paginação ---
releases-pagination-label = Paginação
releases-pagination-prev = « Anterior
releases-pagination-next = Seguinte »
releases-count = { $count ->
    [one] { $count } release
   *[other] { $count } releases
}

# --- Estado do release ---
release-health-title = Estado dos Releases
release-health-heading = Estado dos releases
release-health-sessions-heading = Sessões ao longo do tempo
release-health-period-label = Intervalo de tempo
release-health-period-1h = Última hora
release-health-period-24h = Últimas 24h
release-health-period-7d = Últimos 7 dias
release-health-period-14d = Últimos 14 dias
release-health-period-30d = Últimos 30 dias
release-health-period-90d = Últimos 90 dias
release-health-empty = Nenhum dado de sessão disponível. Os eventos de sessão com um campo <code class="text-mono">status</code> aparecerão aqui.
release-health-col-release = Release
release-health-col-sessions = Sessões
release-health-col-ok = OK
release-health-col-crashed = Com falha
release-health-col-errored = Com erros
release-health-col-crash-free-sessions = Sessões sem falhas
release-health-col-error-free-sessions = Sessões sem erros
release-health-col-crash-free-users = Utilizadores sem falhas
release-health-subtitle = Os resultados de sessão são sinais de saúde reportados pelo SDK, não eventos de erro. Clique numa release para ver os seus problemas.
release-health-crashed-title = Ver problemas desta release
release-health-errored-title = Ver problemas desta release
release-health-errored-hint = A contagem «com erros» são sinais de saúde de sessão reportados pelo SDK (uma sessão que registou um erro tratado mas não falhou), não eventos de erro individuais, e não pode ser listada por sessão. Os problemas ligados são os grupos de erros vistos nesta release.

# --- Detalhe da release (por versão) ---
release-detail-sessions-heading = Estado das sessões
release-detail-sessions-note = Resultados de sessão reportados pelo SDK (ok / com erros / com falhas). São sinais de saúde, não eventos de erro individuais.
release-detail-no-health = Sem dados de sessão para esta release.
release-detail-issues-heading = Problemas nesta release
release-detail-issues-note = Grupos de erros distintos vistos pela primeira ou última vez com esta release.
release-detail-no-issues = Nenhum problema registado para esta release.
release-health-na = n/d
