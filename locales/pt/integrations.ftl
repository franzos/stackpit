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
integrations-email-smtp-hint = O SMTP usa a ligação [email.smtp] do servidor; não é necessário um token por integração.
