# Interface de eventos: a lista de eventos entre projetos e a página de detalhe
# do evento. event-detail-exception-stacktrace contém um &amp; inline e é
# renderizado com |safe. As strings com contagem usam plurais tv_count.

# --- Labels partilhados (lista de eventos + detalhe do evento) ---
events-label-title = Título
events-label-type = Tipo
events-label-level = Nível
events-label-platform = Plataforma
events-label-environment = Ambiente
events-label-time = Hora
events-label-value = Valor

# --- Paginação (partilhada) ---
events-pagination-label = Paginação
events-pagination-prev = « Anterior
events-pagination-next = Seguinte »

# --- Sufixo do título da página (títulos com prefixo dinâmico) ---
events-title-suffix = — Stackpit

# --- Lista de eventos ---
events-list-title = Eventos — Stackpit
events-heading = Eventos
events-list-search-placeholder = Pesquisar eventos…
events-list-search-label = Pesquisar eventos
events-list-select = Selecionar evento
events-list-filter-level = Filtrar por nível
events-list-level-all = Todos os níveis
events-list-filter-type = Filtrar por tipo
events-list-type-all = Todos os tipos
events-list-project-placeholder = ID do projeto
events-list-filter-project = Filtrar por projeto
events-list-filter-submit = Filtrar
events-list-empty = Nenhum evento corresponde aos filtros atuais.
events-untitled = (sem título)
events-col-project = Projeto

# --- Ações em massa ---
events-bulk-delete = Eliminar
events-bulk-delete-selected-confirm = Eliminar os eventos selecionados?
events-bulk-delete-all = Eliminar todos os { $count } correspondentes
events-bulk-delete-all-confirm = { $count ->
    [one] Eliminar permanentemente { $count } evento correspondente?
   *[other] Eliminar permanentemente todos os { $count } eventos correspondentes?
}

# --- Contagem (paginação) ---
events-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventos
}

# --- Detalhe do evento ---
event-detail-event = Evento
event-detail-event-id-label = event_id:
event-detail-nav-label = Navegação de eventos
event-detail-nav-newer = « Mais recente
event-detail-nav-older = Mais antigo »
event-detail-nav-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventos
}
event-detail-nav-in-issue = no problema
event-detail-user-feedback = Feedback do utilizador
event-detail-anonymous = Anónimo
event-detail-related-event = Evento relacionado:
event-detail-exception-stacktrace = Exceção &amp; Stacktrace
event-detail-handled = tratado
event-detail-unhandled = não tratado
event-detail-in = em
event-detail-var-name = Variável
event-detail-no-source = Nenhum contexto de código-fonte disponível
event-detail-breadcrumbs = Rastos
event-detail-th-category = Categoria
event-detail-th-message = Mensagem
event-detail-tags = Tags
event-detail-contexts = Contextos
event-detail-request = Pedido
event-detail-headers = Cabeçalhos
event-detail-th-header = Cabeçalho
event-detail-query-string = Query string
event-detail-body = Corpo
event-detail-user-reports = Relatórios de utilizadores
event-detail-attachments = Anexos
event-detail-att-filename = Nome do ficheiro
event-detail-att-size = Tamanho
event-detail-download = Transferir
event-detail-web-vitals = Web Vitals
event-detail-raw-json = JSON em bruto
event-detail-props-heading = Propriedades do evento
event-detail-prop-event-id = ID do evento
event-detail-prop-timestamp = Data/hora
event-detail-prop-transaction = Transação
event-detail-prop-release = Release
event-detail-prop-server = Servidor
event-detail-prop-sdk = SDK
event-detail-prop-received = Recebido
event-detail-user-heading = Utilizador
event-detail-user-id = ID
event-detail-user-email = E-mail
event-detail-user-username = Nome de utilizador
event-detail-user-ip = Endereço IP

# --- Relatórios de cliente (eventos descartados) ---
# Reutiliza events-untitled e events-pagination-* (partilhados, mesma ficheiro).
client-reports-title = Relatórios de cliente
client-reports-heading = Relatórios de cliente
client-reports-dropped-heading = Eventos descartados
client-reports-dropped-subtitle = O que os SDKs descartaram antes de enviar, por categoria e motivo.
client-reports-th-category = Categoria
client-reports-th-reason = Motivo
client-reports-th-reasons = Motivos
client-reports-th-dropped = Descartados
client-reports-empty = Nenhum relatório de cliente encontrado para este projeto.
client-reports-reports-heading = Relatórios
client-reports-delete = Eliminar
client-reports-delete-selected-confirm = Eliminar os relatórios selecionados?
client-reports-th-event-id = ID do evento
client-reports-th-title = Título
client-reports-th-timestamp = Data/hora
client-reports-th-platform = Plataforma
client-reports-th-release = Release
client-reports-select = Selecionar relatório
client-reports-delete-all = Eliminar todos os { $count }
client-reports-delete-all-confirm = { $count ->
    [one] Eliminar { $count } relatório correspondente?
   *[other] Eliminar todos os { $count } relatórios correspondentes?
}
client-reports-count = { $count ->
    [one] { $count } relatório
   *[other] { $count } relatórios
}

# --- Relatórios de utilizadores (feedback do utilizador) ---
# Reutiliza events-untitled e events-pagination-* (partilhados, mesma ficheiro).
user-reports-title = Relatórios de utilizadores
user-reports-heading = Relatórios de utilizadores
user-reports-empty = Nenhum relatório de utilizador encontrado para este projeto.
user-reports-delete = Eliminar
user-reports-delete-selected-confirm = Eliminar os relatórios selecionados?
user-reports-th-event-id = ID do evento
user-reports-th-title = Título
user-reports-th-timestamp = Data/hora
user-reports-th-platform = Plataforma
user-reports-th-release = Release
user-reports-select = Selecionar relatório
user-reports-delete-all = Eliminar todos os { $count }
user-reports-delete-all-confirm = { $count ->
    [one] Eliminar { $count } relatório correspondente?
   *[other] Eliminar todos os { $count } relatórios correspondentes?
}
user-reports-count = { $count ->
    [one] { $count } relatório
   *[other] { $count } relatórios
}
