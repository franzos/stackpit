# Interface de projetos: lista, novo, definições (geral/chaves/source maps/
# filtros), integrações e a confirmação de criação. Os valores renderizados com
# |safe contêm markup HTML inline; as tags permanecem idênticas, só o texto é
# traduzido.

# --- Lista de projetos ---
projects-list-title = Projetos — Stackpit
projects-list-heading = Projetos
projects-list-subtitle = Monitorize o estado de toda a sua arquitetura.
projects-list-all-events = Todos os eventos
projects-list-all-releases = Todos os releases
projects-list-new = + Novo projeto
projects-list-search-placeholder = Pesquisar projetos por nome, plataforma ou proprietário…
projects-list-search-label = Pesquisar projetos
projects-list-filter = Filtrar
projects-org-filter-label = Filtrar por organização
projects-org-filter-all = Todas as organizações
projects-list-empty = Nenhum projeto encontrado. Os eventos aparecerão aqui assim que forem recebidos.
projects-period-label = Intervalo de tempo
projects-col-project = Projeto
projects-col-platforms = Plataformas
projects-col-issues = Problemas
projects-col-events = Eventos
projects-col-breakdown = Repartição
projects-col-release = Release
projects-col-first-seen = Visto pela primeira vez
projects-col-last-seen = Visto pela última vez
projects-breakdown-errors = Erros:
projects-breakdown-transactions = Transações:
projects-breakdown-sessions = Sessões:
projects-breakdown-other = Outros:
projects-legend-errors = Erros
projects-legend-transactions = Transações
projects-legend-sessions = Sessões
projects-legend-other = Outros

# --- Partilhado nos formulários de projeto ---
projects-optional = (opcional)
projects-cancel = Cancelar
projects-remove = Remover
projects-delete = Eliminar
projects-name-placeholder = O Meu Projeto

# --- Novo projeto ---
projects-new-title = Novo projeto — Stackpit
projects-new-heading = Novo projeto
projects-new-name-label = Nome do projeto
projects-new-platform-label = Plataforma
projects-new-platform-select = Selecione uma plataforma…
projects-new-platform-other = Outra
projects-new-platform-native = Nativo (C/C++)
projects-new-submit = Criar projeto

# --- Tabs de definições (partilhados pelas páginas de definições) ---
projects-tab-general = Geral
projects-tab-sdk = Configuração do SDK
projects-tab-sourcemaps = Source maps
projects-tab-filters = Filtros
projects-tab-integrations = Integrações

# --- Definições: geral ---
projects-settings-heading = Definições
projects-settings-archived = (arquivado)
projects-settings-name-heading = Nome do projeto
projects-settings-display-name = Nome a apresentar
projects-settings-save-name = Guardar nome
projects-settings-info-heading = Informação do projeto
projects-settings-status = Estado
projects-settings-source = Origem
projects-repos-heading = Repositórios de código-fonte
projects-repos-help = Associe frames da pilha ao código-fonte na sua forge. Registe um release com um SHA de commit através de <code class="text-mono">sentry-cli</code> para ativar as ligações.
projects-repos-empty = Nenhum repositório configurado.
projects-repos-url-label = URL do repositório
projects-repos-col-forge = Forge
projects-repos-template = Modelo de URL
projects-repos-auto = automático
projects-repos-remove-confirm = Remover este repositório?
projects-repos-add = Adicionar repositório
projects-repos-add-help = Adiciona ligações de código-fonte clicáveis (por ex. "Ver no GitHub") junto aos frames da pilha. Requer um release com um SHA de commit — o tipo de forge é detetado automaticamente. Suportados: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. Para outras forges, indique um modelo de URL.
projects-danger-heading = Zona de perigo
projects-archive-desc = Arquive este projeto. Os projetos arquivados rejeitam novos eventos.
projects-archive-confirm = Arquivar este projeto? Os novos eventos serão rejeitados.
projects-archive-submit = Arquivar projeto
projects-unarchive-desc = Desarquive este projeto para voltar a aceitar eventos.
projects-unarchive-submit = Desarquivar projeto
projects-delete-desc = Elimine permanentemente este projeto e todos os seus dados. Isto não pode ser anulado.
projects-delete-confirm = Eliminar este projeto e TODOS os seus dados? Isto não pode ser anulado.
projects-delete-submit = Eliminar projeto
projects-move-heading = Mover para outra organização
projects-move-desc = Mova este projeto para outra organização de que é proprietário. Os seus dados e DSNs continuam válidos, mas as integrações de notificação são desassociadas e têm de ser adicionadas novamente na nova organização.
projects-move-target-label = Organização de destino
projects-move-confirm-pre = Escreva
projects-move-confirm-post = para confirmar.
projects-move-confirm-placeholder = Nome do projeto
projects-move-confirm-dialog = Mover este projeto para a organização selecionada?
projects-move-submit = Mover projeto
projects-move-err-invalid-target = Organização de destino inválida.
projects-move-err-name-mismatch = O nome do projeto não corresponde.
projects-move-err-denied = Não é proprietário da organização de destino.
projects-move-err-conflict = Não foi possível mover o projeto; pode ter sido alterado. Tente novamente.

# --- Definições: configuração do SDK / chaves ---
projects-keys-title = Configuração do SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = Nenhuma chave registada. Crie uma chave abaixo para obter uma DSN.
projects-keys-list-heading = Chaves do projeto
projects-keys-empty = Nenhuma chave registada para este projeto.
projects-keys-col-public = Chave pública
projects-keys-col-label = Etiqueta
projects-keys-col-status = Estado
projects-keys-col-created = Criada
projects-keys-delete-confirm = Eliminar esta chave? Os SDKs que a usam deixarão de funcionar.
projects-keys-create-heading = Criar chave
projects-keys-label-label = Etiqueta
projects-keys-label-placeholder = por ex. production, staging
projects-keys-create-submit = Criar chave

# --- Definições: source maps ---
projects-sourcemaps-title = Source Maps
projects-sourcemaps-apikey-heading = Chave de API
projects-sourcemaps-apikey-desc = O carregamento de source maps requer uma chave de API. Específica deste projeto e utilizável apenas para operações de source maps.
projects-sourcemaps-key-generated = Chave gerada:
projects-sourcemaps-key-warning = Copie esta chave agora — não voltará a ser mostrada.
projects-sourcemaps-col-key = Chave
projects-sourcemaps-regen-confirm = Regenerar a chave? A chave atual deixará de funcionar.
projects-sourcemaps-regen = Regenerar
projects-sourcemaps-empty = Nenhuma chave de API de source maps para este projeto.
projects-sourcemaps-generate = Gerar chave
projects-sourcemaps-setup-heading = Configuração
projects-sourcemaps-setup-desc = Utilize <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> para carregar source maps. Defina estas variáveis de ambiente:
projects-sourcemaps-then-upload = Depois carregue:

# --- Definições: filtros ---
projects-filters-inbound-heading = Filtros de entrada
projects-filters-inbound-desc = Filtros integrados que descartam eventos que correspondem a padrões de ruído comuns.
projects-filters-browser-ext = Extensões do navegador — descartar eventos de extensões do Chrome/Firefox/Safari
projects-filters-localhost = Localhost — descartar eventos de localhost, 127.0.0.1, IPs privados
projects-filters-inbound-submit = Guardar filtros de entrada
projects-filters-message-heading = Filtros de mensagem
projects-filters-message-help = Padrões glob comparados com os títulos dos eventos. Utilize <code class="text-mono">*</code> para qualquer sequência, <code class="text-mono">?</code> para um único caráter.
projects-filters-col-pattern = Padrão
projects-filters-message-empty = Nenhum filtro de mensagem configurado.
projects-filters-add-pattern = Adicionar padrão
projects-filters-message-submit = Adicionar filtro de mensagem
projects-filters-ratelimit-heading = Limite de taxa
projects-filters-ratelimit-desc = Máximo de eventos por minuto para este projeto. 0 = ilimitado.
projects-filters-ratelimit-label = Eventos por minuto
projects-filters-ratelimit-submit = Guardar limite de taxa
projects-filters-env-heading = Ambientes excluídos
projects-filters-env-desc = Os eventos destes ambientes serão descartados silenciosamente.
projects-filters-col-environment = Ambiente
projects-filters-env-empty = Nenhum ambiente excluído.
projects-filters-env-add-label = Adicionar ambiente excluído
projects-filters-env-submit = Excluir ambiente
projects-filters-release-heading = Filtros de release
projects-filters-release-desc = Padrões glob comparados com as versões de release. Os eventos correspondentes são descartados.
projects-filters-release-empty = Nenhum filtro de release.
projects-filters-release-submit = Adicionar filtro de release
projects-filters-ua-heading = Filtros de user-agent
projects-filters-ua-desc = Padrões glob comparados com os cabeçalhos User-Agent. Os padrões integrados para kube-probe e verificadores de saúde estão sempre ativos.
projects-filters-ua-empty = Nenhum filtro de user-agent personalizado.
projects-filters-ua-submit = Adicionar filtro de user-agent
projects-filters-rules-heading = Regras personalizadas
projects-filters-rules-desc = Regras avançadas que correspondem a campos de eventos. As regras de maior prioridade são avaliadas primeiro.
projects-filters-col-field = Campo
projects-filters-col-operator = Operador
projects-filters-col-value = Valor
projects-filters-col-action = Ação
projects-filters-col-priority = Prioridade
projects-filters-rules-empty = Nenhuma regra personalizada.
projects-filters-sample-rate-label = Taxa de amostragem
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Adicionar regra
projects-filters-op = { $op ->
    [not_equals] diferente de
    [contains] contém
    [not_contains] não contém
    [starts_with] começa com
    [in] em
    [not_in] não em
   *[equals] igual a
}
projects-filters-action = { $action ->
    [sample] amostrar
   *[drop] descartar
}
projects-filters-ip-heading = Lista de bloqueio de IP
projects-filters-ip-desc = Blocos CIDR ou IPs individuais. Os eventos de IPs bloqueados são descartados silenciosamente.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = Nenhum bloco de IP configurado.
projects-filters-ip-add-label = Adicionar CIDR
projects-filters-ip-submit = Bloquear intervalo de IP
projects-filters-discard-heading = Estatísticas de descartes
projects-filters-discard-window = (últimos 7 dias)
projects-filters-col-date = Data
projects-filters-col-reason = Motivo
projects-filters-col-count = Contagem

# Labels de entidade de filtro, interpolados em flash-not-found-filter ao eliminar.
projects-filter-label-message = filtro de mensagem
projects-filter-label-environment = filtro de ambiente
projects-filter-label-release = filtro de release
projects-filter-label-user-agent = filtro de user-agent
projects-filter-label-rule = regra de filtro

# --- Definições: integrações ---
projects-integrations-active-heading = Integrações ativas
projects-integrations-active-empty = Nenhuma integração ativada. Adicione primeiro uma integração global na página <a class="text-primary" href="/web/settings/integrations/">Integrações</a> e ative-a aqui. Pode limitar cada uma por nível mínimo e ambiente, para que o ruído de dev não chegue aos canais de produção.
projects-integrations-deactivate-confirm = Desativar esta integração para o projeto?
projects-integrations-deactivate = Desativar
projects-integrations-notify-new-issues = Novos problemas
projects-integrations-notify-regressions = Regressões
projects-integrations-notify-threshold = Alertas de limiar
projects-integrations-notify-digests = Resumos
projects-integrations-min-level = Nível mínimo
projects-integrations-level-any = Qualquer
projects-integrations-env-filter = Filtro de ambiente
projects-integrations-env-placeholder = por ex. production
projects-integrations-to-address = Endereço de destino
projects-integrations-to-address-note = (apenas integrações de e-mail)
projects-integrations-activate-heading = Ativar integração
projects-integrations-integration-label = Integração
projects-integrations-activate-submit = Ativar
projects-integrations-available-empty = Nenhuma integração disponível. <a class="text-primary" href="/web/settings/integrations/">Crie uma primeiro</a>.

# --- Projeto criado ---
projects-created-word = criado
projects-created-breadcrumb = Criado
projects-created-heading = Projeto criado
projects-created-subtitle = Utilize a DSN abaixo para configurar o seu SDK.
projects-created-settings-btn = Definições do projeto
projects-created-back = Voltar aos projetos
projects-created-details-heading = Detalhes do projeto
projects-created-col-id = ID do projeto
projects-created-sdk-desc-before = Instale o SDK do Sentry para
projects-created-sdk-desc-after = e inicialize-o com a DSN acima.
projects-created-docs-javascript = Documentação Sentry JavaScript →
projects-created-docs-python = Documentação Sentry Python →
projects-created-docs-rust = Documentação Sentry Rust →
projects-created-docs-go = Documentação Sentry Go →
projects-created-docs-node = Documentação Sentry Node.js →
projects-created-docs-java = Documentação Sentry Java →
projects-created-docs-ruby = Documentação Sentry Ruby →
projects-created-docs-php = Documentação Sentry PHP →
projects-created-docs-elixir = Documentação Sentry Elixir →
projects-created-docs-dotnet = Documentação Sentry .NET →
projects-created-docs-apple = Documentação Sentry Apple →
projects-created-docs-kotlin = Documentação Sentry Kotlin →
projects-created-docs-native = Documentação Sentry Native →
projects-created-docs-generic = Documentação da plataforma Sentry →
projects-repos-forge-override = Tipo de forge
projects-repos-forge-detected = Detetado automaticamente
projects-repos-forge-override-help = Define isto só se o tipo detetado estiver errado — normalmente numa instância auto-alojada cujo nome de anfitrião nada diz sobre a forge que executa.
projects-repos-prefix = Prefixo do caminho
projects-repos-prefix-placeholder = services/api/
projects-repos-prefix-help = Que frames da pilha pertencem a este repositório, comparados com o início do nome de ficheiro de um frame. Deixa vazio num projeto com um só repositório. Assim que um repositório aqui tiver prefixo, só a correspondência por prefixo se aplica e os repositórios sem prefixo deixam de produzir ligações para o código.
projects-repos-col-prefix = Prefixo do caminho
projects-integrations-reset = Repor os valores da organização
projects-integrations-reset-confirm = Descartar as definições deste projeto e voltar a entregar com os valores da organização?
projects-integrations-global-hint = Esta integração entrega a todos os projetos da organização. As definições abaixo apenas a personalizam aqui; para parar a entrega por completo, exclui este projeto na página da integração.
projects-integrations-tracker-hint = O repositório de destino vem das definições de repositório deste projeto, não daqui.
projects-repos-inert = forja desconhecida
projects-repos-inert-help = O Stackpit não conseguiu determinar que forja corre neste anfitrião, por isso o repositório não produz ligações para o código nem corresponde a nenhum issue tracker. Escolhe a forja abaixo e guarda.
projects-integrations-activate-tracker-heading = Ativar issue tracker
projects-integrations-activate-tracker-help = Permite que este projeto abra issues quando for preciso. O repositório de destino vem dos repositórios do projeto, por isso não há mais nada a definir aqui.
