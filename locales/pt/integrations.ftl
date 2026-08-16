# Interface de definições de integrações: a lista (templates/integrations.html) e
# os três formulários de "adicionar" (webhook, slack, e-mail). Reutiliza
# nav-settings/nav-integrations para o chrome. Os espaços separadores estão no
# template. integrations-empty contém markup <strong> inline e o glifo de seta,
# renderizado com |safe.
integrations-page-title = Integrações — Stackpit
integrations-subtitle = Saídas de webhook, Slack e e-mail. O encaminhamento por projeto é definido nas definições de cada projeto.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + E-mail
integrations-license-required-badge = Requer licença
integrations-empty = Ainda não há integrações. Adicione uma acima para começar a receber notificações. Depois de adicionar, ative-a por projeto em <strong>Definições do projeto → Integrações</strong>.
integrations-col-name = Nome
integrations-col-type = Tipo
integrations-col-endpoint = Endpoint
integrations-col-created = Criada
integrations-delete-confirm = Eliminar esta integração? Será removida de todos os projetos.
integrations-test = Testar
integrations-delete = Eliminar
flash-test-failed = Teste falhou: { $error }

# Labels/botões partilhados pelos três formulários de adicionar integração.
integrations-cancel = Cancelar
integrations-optional = (opcional)
integrations-required = (obrigatório)
integrations-create = Criar integração

# --- Adicionar webhook ---
integrations-webhook-title = Adicionar webhook — Stackpit
integrations-webhook-breadcrumb = Adicionar webhook
integrations-webhook-heading = Adicionar integração de webhook
integrations-webhook-name-placeholder = por ex. Alertas de produção
integrations-webhook-url-label = URL do webhook
integrations-webhook-secret-label = Segredo HMAC
integrations-webhook-secret-placeholder = Segredo de assinatura opcional

# --- Adicionar Slack ---
integrations-slack-title = Adicionar Slack — Stackpit
integrations-slack-breadcrumb = Adicionar Slack
integrations-slack-heading = Adicionar integração do Slack
integrations-slack-name-placeholder = por ex. canal #alerts
integrations-slack-url-label = URL do webhook do Slack

# --- Adicionar e-mail ---
integrations-email-title = Adicionar e-mail — Stackpit
integrations-email-breadcrumb = Adicionar e-mail
integrations-email-heading = Adicionar integração de e-mail
integrations-email-name-placeholder = por ex. Alertas por e-mail da equipa
integrations-email-lock-pre = O fornecedor e o remetente provêm da
integrations-email-lock-post = configuração do servidor; esta integração apenas escolhe o destinatário.
integrations-email-provider-label = Fornecedor
integrations-email-token-label = Token de API
integrations-email-token-placeholder-default = Deixe em branco para usar o predefinido
integrations-email-token-placeholder = Token de API do fornecedor
integrations-email-from-label = Endereço de remetente
integrations-email-fromname-label = Nome do remetente
integrations-email-smtp-hint = O SMTP usa a ligação [email] do servidor; não é necessário um token por integração.

# Gestor de issues
integrations-add-tracker = + Gestor de issues
integrations-tracker-title = Adicionar gestor de issues — Stackpit
integrations-tracker-breadcrumb = Adicionar gestor de issues
integrations-tracker-heading = Adicionar integração de gestor de issues
integrations-tracker-kind-label = Gestor
integrations-tracker-name-placeholder = por exemplo GitHub Issues
integrations-tracker-url-label = URL base
integrations-tracker-token-label = Token de API
integrations-tracker-token-placeholder = Token de acesso pessoal
integrations-tracker-target-help = O repositório de destino vem das definições de repositório de cada projeto, por isso não se configura aqui. Adiciona o repositório nas definições do projeto.
integrations-global-label = Entregar a todos os projetos
integrations-global-help = Os alertas vão para todos os projetos desta organização, exceto os que excluíres na página desta integração. Os filtros de nível e ambiente por projeto continuam a aplicar-se por cima.
integrations-global-badge = toda a organização
integrations-global-save = Guardar encaminhamento
integrations-global-on = Entregar a toda a organização
integrations-global-off = Parar de entregar a toda a organização

# Detalhe da integração: encaminhamento por projeto
integrations-detail-title = Integração — Stackpit
integrations-back = Voltar às integrações
integrations-projects-heading = Encaminhamento por projeto
integrations-projects-hint-global = Esta integração entrega a todos os projetos abaixo, a não ser que a excluas. Excluir é a única forma de sair; não existe lista de inclusão.
integrations-projects-hint-per-project = Esta integração só entrega onde um projeto a tiver ativado. Marca-a para toda a organização se quiseres que entregue em todo o lado.
integrations-projects-hint-tracker = Os gestores de issues são associados aos repositórios de um projeto por forge e por anfitrião. Excluir um projeto deixa este gestor fora das suas opções de criação.
integrations-projects-empty = Esta organização ainda não tem projetos.
integrations-col-project = Projeto
integrations-col-state = Estado
integrations-project-archived = arquivado
integrations-state-default = A entregar
integrations-state-customised = Personalizado
integrations-state-excluded = Excluído
integrations-state-no-repo = Sem repositório correspondente
integrations-state-not-routed = Não ativado
integrations-exclude = Excluir
integrations-include = Incluir
integrations-email-to-label = Destinatário predefinido
integrations-email-to-help = Usado onde um projeto não tiver definido o seu próprio endereço. Obrigatório numa integração para toda a organização.
