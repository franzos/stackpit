# Interface de definições: a página de predefinições do navegador
# (templates/browser_defaults.html, chaves defaults-*) e a página autónoma de
# provisionamento de organizações (templates/provision.html, chaves provision-*).
# Reutiliza nav-settings. Os valores de nível (fatal/error/...) permanecem
# literais no template, como nas interfaces de problemas/eventos, onde os níveis
# de log se mantêm em inglês canónico.

# --- Predefinições do navegador ---
defaults-page-title = Predefinições do navegador — Stackpit
defaults-subtitle = Defina valores de filtro predefinidos para as páginas de lista. Guardado como cookie do navegador.
defaults-none = Sem predefinição
defaults-status-label = Estado predefinido (problemas)
defaults-status-unresolved = Por resolver
defaults-status-resolved = Resolvido
defaults-status-ignored = Ignorado
defaults-level-label = Nível predefinido
defaults-period-label = Intervalo de tempo predefinido
defaults-save = Guardar predefinições
defaults-clear-confirm = Limpar todas as predefinições do navegador?
defaults-clear = Limpar todas as predefinições
flash-defaults-saved = Predefinições guardadas
flash-defaults-cleared = Predefinições limpas

# --- Idioma preferido ---
settings-language-heading = Idioma preferido
settings-language-subtitle = Escolha o idioma da interface do Stackpit. As contas com sessão iniciada mantêm-no em todos os dispositivos.
settings-language-label = Idioma
settings-language-save = Guardar idioma

settings-aria-sections = Secções de definições

# --- Página de provisionamento (página autónoma) ---
provision-page-title = Configurar organizações — Stackpit
provision-heading = Configurar organizações
provision-subtitle-1 = As seguintes organizações estão disponíveis a partir do seu fornecedor de identidade.
provision-subtitle-2 = Selecione as que pretende criar no Stackpit.
provision-create = Criar selecionadas
provision-skip = Ignorar

# Fila de entrega
queue-page-title = Fila de entrega — Stackpit
queue-subtitle = Notificações que não foi possível entregar. São repetidas automaticamente durante 24 horas e depois ficam aqui à tua espera.
queue-count-pending = { $count } pendentes
queue-count-failed = { $count } falhadas
queue-empty = Nada em fila. Todas as notificações foram entregues.
queue-col-integration = Integração
queue-col-project = Projeto
queue-col-state = Estado
queue-col-attempts = Tentativas
queue-col-queued = Em fila desde
queue-col-error = Último erro
queue-state-pending = A repetir
queue-state-failed = Desistiu
queue-replay = Reenviar
queue-cancel = Descartar
queue-cancel-confirm = Descartar esta notificação sem a entregar?
