# Equivalentes em português a locales/en/errors.ftl. O nome da marca "Stackpit"
# permanece literal nos templates, como em base.html/login.html.
error-page-title = Erro - Stackpit
error-heading = Erro
error-back-projects = Voltar aos projetos

# Página de confirmação de convite criado (apenas inglês/locale predefinido).
invite-created-page-title = Convite criado - Stackpit
invite-created-heading = Convite criado
invite-created-share = Partilhe esta ligação. É válida durante { $ttl } e de utilização única.
invite-created-back-members = Voltar aos membros

# --- Mensagens de flash, sucesso e validação (dependentes do locale) ---

# Diagnósticos de "não encontrado". O prefixo "Erro:" é anteposto em Rust; o
# valor contém apenas a expressão da entidade mais o id.
flash-not-found-project = projeto não encontrado: { $id }
flash-not-found-key = chave de API não encontrada: { $id }
flash-not-found-integration = integração não encontrada: { $id }
flash-not-found-alert-rule = regra de alerta não encontrada: { $id }
flash-not-found-digest-schedule = agendamento de resumo não encontrado: { $id }
flash-not-found-repo = repositório não encontrado: { $id }
flash-not-found-project-integration = integração de projeto não encontrada: { $id }
flash-not-found-filter = { $label } não encontrado

# Validação das regras de filtro
flash-unrecognized-field = Campo não reconhecido: { $value }
flash-unrecognized-operator = Operador não reconhecido: { $value }
flash-unrecognized-action = Ação não reconhecida: { $value }

# Definições do projeto
flash-project-name-updated = Nome do projeto atualizado
flash-project-name-too-long = O nome do projeto excede o comprimento máximo de { $max } caracteres
flash-repo-url-required = O URL do repositório é obrigatório
flash-repo-url-too-long = O URL do repositório excede o comprimento máximo de 2048 caracteres
flash-repo-added = Repositório adicionado
flash-repo-removed = Repositório removido
flash-project-archived = Projeto arquivado
flash-project-unarchived = Projeto desarquivado
flash-key-created = Chave criada
flash-key-deleted = Chave eliminada

# Alertas e resumos
flash-project-not-found-or-denied = Erro: projeto não encontrado ou acesso negado
flash-alert-rule-created = Regra de alerta criada
flash-alert-rule-deleted = Regra de alerta eliminada
flash-digest-schedule-created = Agendamento de resumo criado
flash-digest-schedule-deleted = Agendamento de resumo eliminado

# Integrações do projeto
flash-integration-not-found = Integração não encontrada
flash-integration-activated = Integração ativada
flash-integration-updated = Integração atualizada
flash-integration-deactivated = Integração desativada

# Integrações da organização
flash-name-required = O nome é obrigatório
flash-invalid-integration-kind = Tipo de integração inválido
flash-invalid-email-provider = Fornecedor de e-mail inválido
flash-api-token-required = O token de API é obrigatório.
flash-from-address-required = O endereço de remetente é obrigatório.
flash-smtp-not-configured = O SMTP não está configurado. Defina [email] host na configuração do servidor.
flash-invalid-to-address = O destinatário tem de ser um endereço de e-mail válido.
flash-test-digest-sent = Resumo de teste em fila para { $count } projeto(s) para as respetivas integrações com resumos ativados.
flash-test-digest-sample = Sem atividade recente, por isso foi colocado em fila um resumo de exemplo identificado.
flash-test-digest-no-target = Nenhuma integração tem os resumos ativados para o projeto deste agendamento.
flash-url-required = O URL é obrigatório
flash-secret-not-configured = Não é possível guardar o segredo: a encriptação não está configurada. Defina STACKPIT_MASTER_KEY para ativar o armazenamento de segredos.
flash-integration-created = Integração criada
flash-integration-name-exists = Já existe uma integração com esse nome.
flash-integration-deleted = Integração eliminada
flash-integration-no-url = A integração não tem nenhum URL configurado
flash-test-notification-sent = Notificação de teste enviada

# Filtros de entrada
flash-inbound-filters-updated = Filtros de entrada atualizados
flash-pattern-required = O padrão é obrigatório
flash-message-filter-added = Filtro de mensagem adicionado
flash-message-filter-removed = Filtro de mensagem removido
flash-rate-limit-updated = Limite de taxa atualizado
flash-environment-required = O ambiente é obrigatório
flash-environment-excluded = Ambiente excluído
flash-environment-filter-removed = Filtro de ambiente removido
flash-release-filter-added = Filtro de release adicionado
flash-release-filter-removed = Filtro de release removido
flash-ua-filter-added = Filtro de user-agent adicionado
flash-ua-filter-removed = Filtro de user-agent removido
flash-rule-added = Regra adicionada
flash-rule-removed = Regra removida
flash-cidr-required = O CIDR é obrigatório
flash-invalid-cidr = Formato de CIDR inválido
flash-ip-block-added = Bloco de IP adicionado
flash-ip-block-removed = Bloco de IP removido

# Novo projeto
flash-project-name-required = O nome do projeto é obrigatório
