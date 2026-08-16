# Interface de problemas: a lista de problemas agrupada por impressão digital e a
# página de detalhe do problema. issue-detail-exception-stacktrace contém um &amp;
# inline e é renderizado com |safe. As strings com contagem usam plurais tv_count.

# --- Labels partilhados (lista de problemas + detalhe do problema) ---
issues-label-title = Título
issues-label-level = Nível
issues-label-events = Eventos
issues-label-users = Utilizadores
issues-label-trend = Tendência
issues-trend-tooltip = Volume de eventos no período selecionado
issues-label-status = Estado
issues-label-first-seen = Visto pela primeira vez
issues-label-last-seen = Visto pela última vez
issues-label-value = Valor

# --- Valores de estado (opções de filtro + badges) ---
issues-status-unresolved = Por resolver
issues-status-resolved = Resolvido
issues-status-ignored = Ignorado

# --- Paginação (partilhada) ---
issues-pagination-label = Paginação
issues-pagination-prev = « Anterior
issues-pagination-next = Seguinte »

# --- Sufixo do título da página (títulos com prefixo dinâmico) ---
issues-title-suffix = — Stackpit

# --- Lista de problemas ---
issues-list-subtitle = Problemas agrupados por impressão digital.
issues-list-filtered-by-tag = Filtrado por tag:
issues-list-clear-tag = Limpar filtro de tag
issues-list-search-placeholder = Pesquisar problemas…
issues-list-search-label = Pesquisar problemas
issues-list-select = Selecionar problema
issues-list-filter-status = Filtrar por estado
issues-list-status-all = Todos os estados
issues-list-filter-level = Filtrar por nível
issues-list-level-all = Todos os níveis
issues-list-filter-release = Filtrar por release
issues-list-release-all = Todos os releases
issues-list-filter-environment = Filtrar por ambiente
issues-list-environment-all = Todos os ambientes
issues-period-label = Intervalo de tempo
issues-list-filter-submit = Filtrar
issues-list-empty = Nenhum problema corresponde aos filtros atuais.
issues-untitled = (sem título)

# --- Ações em massa ---
issues-bulk-resolve-all = Resolver todos os { $count }
issues-bulk-ignore-all = Ignorar todos os { $count }
issues-bulk-delete-all = Eliminar todos os { $count }
issues-bulk-resolve-confirm = { $count ->
    [one] Resolver { $count } problema correspondente?
   *[other] Resolver todos os { $count } problemas correspondentes?
}
issues-bulk-ignore-confirm = { $count ->
    [one] Ignorar { $count } problema correspondente?
   *[other] Ignorar todos os { $count } problemas correspondentes?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] Eliminar permanentemente { $count } problema correspondente?
   *[other] Eliminar permanentemente todos os { $count } problemas correspondentes?
}
issues-bulk-resolve = Resolver
issues-bulk-ignore = Ignorar
issues-bulk-delete = Eliminar
issues-bulk-delete-selected-confirm = Eliminar permanentemente os problemas selecionados?

# --- Contagem (paginação) ---
issues-count = { $count ->
    [one] { $count } problema
   *[other] { $count } problemas
}

# --- Detalhe do problema ---
issue-detail-title-fallback = Problema
issue-detail-resolve = ✓ Resolver
issue-detail-reopen = Reabrir
issue-detail-unignore = Deixar de ignorar
issue-detail-tab-details = Detalhes
issue-detail-tab-events = Todos os eventos
issue-detail-exception-stacktrace = Exceção &amp; Stacktrace
issue-detail-handled = tratado
issue-detail-unhandled = não tratado
issue-detail-in = em
issue-detail-var-name = Variável
issue-detail-no-source = Nenhum contexto de código-fonte disponível
issue-detail-in-app-only = Apenas frames da aplicação
issue-detail-reverse-order = Inverter a ordem
issue-detail-copy = Copiar
issue-detail-copy-frame = Copiar este frame
issue-detail-library-frames = { $count ->
    [one] { $count } frame de biblioteca
   *[other] { $count } frames de biblioteca
}
issue-detail-minified-hint = Estes frames parecem minificados e nenhuma source map foi aplicada.
issue-detail-minified-hint-link = Carregar source maps
issue-detail-breadcrumbs = Rastos
issue-detail-th-time = Hora
issue-detail-th-category = Categoria
issue-detail-th-message = Mensagem
issue-detail-crumb-data = dados
issue-detail-crumb-filter = Filtrar breadcrumbs por tipo
issue-detail-crumb-filter-all = Todos os tipos
issue-detail-tags = Tags
issue-detail-contexts = Contextos
issue-detail-additional-data = Dados adicionais
issue-detail-view-replay = Ver replay
issue-detail-view-trace = Ver trace
issue-detail-request = Pedido
issue-detail-headers = Cabeçalhos
issue-detail-th-header = Cabeçalho
issue-detail-query-string = Query string
issue-detail-body = Corpo
issue-detail-environment = Ambiente
issue-detail-user-reports = Relatórios de utilizadores
issue-detail-anonymous = Anónimo
issue-detail-attachments = Anexos
issue-detail-att-filename = Nome do ficheiro
issue-detail-att-type = Tipo
issue-detail-att-size = Tamanho
issue-detail-download = Transferir
issue-detail-raw-json = JSON em bruto
issue-detail-no-events = Nenhum evento encontrado para este problema.
issue-detail-ev-id = ID do evento
issue-detail-ev-timestamp = Data/hora
issue-detail-ev-platform = Plataforma
issue-detail-events-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventos
}
issue-detail-props-heading = Propriedades do problema
issue-detail-fingerprint = Impressão digital
issue-detail-tag-facets = Facetas de tags
issue-detail-discard-undo-title = Voltar a aceitar futuros eventos com esta impressão digital
issue-detail-discard-undo = Anular descarte
issue-detail-discard-confirm = Descartar todos os futuros eventos com esta impressão digital?
issue-detail-discard-title = Descartar silenciosamente os futuros eventos com esta impressão digital
issue-detail-discard = Descartar futuros eventos
issue-detail-create-external-issue = Criar issue
issue-detail-external-tracker = Gestor externo
issue-detail-view-on = Ver em
flash-tracker-create-failed = Não foi possível criar a issue. Verifica o token e o repositório da integração e tenta novamente.
flash-tracker-config-incomplete = Falta um repositório ou um token a esta integração. Corrige isso nas definições da integração.
issue-detail-external-unlink = Desassociar
issue-detail-external-unlink-confirm = Remover esta ligação? A issue permanece na forge — fecha-a ou apaga-a lá.
issue-detail-external-orphaned = integração removida
flash-tracker-unlinked = Ligação removida. A issue continua a existir na forge.
flash-tracker-ambiguous = Este projeto tem mais do que um repositório onde este gestor pode criar a issue. Escolhe um e tenta novamente.
