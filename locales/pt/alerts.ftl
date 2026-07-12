# Página de alertas e resumos (templates/alerts.html). Reutiliza nav-settings e
# nav-alerts-digests para os elementos de chrome. Os espaços separadores estão no
# template, por isso os valores não têm espaços à esquerda/direita.
# alerts-page-title mantém a entidade &amp; e é renderizado com |safe.
alerts-page-title = Alertas &amp; resumos — Stackpit
alerts-notify-help-pre = As notificações são enviadas através das
alerts-notify-help-post = configuradas.

# --- Tipos de notificação ---
alerts-notify-types-heading = Tipos de notificação
alerts-notify-types-desc = Os alertas de novo problema e regressão disparam a cada problema recém-visto ou reincidente, controlados por integração abaixo. As regras de limiar disparam pelo volume de eventos numa janela; os resumos são sínteses periódicas.
alerts-notify-types-empty = Ainda não há integrações de projeto ativas. Vincule uma na página de integrações de um projeto.
alerts-col-integration = Integração
alerts-col-new-issues = Novos problemas
alerts-col-regressions = Regressões
alerts-col-digests = Resumos
alerts-notify-save = Guardar

# --- Regras de limiar ---
alerts-threshold-heading = Regras de limiar
alerts-threshold-desc = Dispara quando um problema recebe mais de N eventos numa janela temporal.
alerts-rules-empty = Ainda não há regras de alerta.
alerts-col-scope = Âmbito
alerts-col-issue = Problema
alerts-col-threshold = Limiar
alerts-col-window = Janela
alerts-col-cooldown = Tempo de espera
alerts-scope-global = Global
alerts-fingerprint-any = Qualquer
alerts-rule-delete-confirm = Eliminar esta regra de alerta?
alerts-delete-label = Eliminar
alerts-add-rule = + Adicionar regra de alerta
alerts-all-projects = Todos os projetos
alerts-project-fallback = Projeto { $id }
alerts-fingerprint-label = Impressão digital do problema
alerts-fingerprint-hint = (vazio = qualquer)
alerts-fingerprint-placeholder = qualquer problema
alerts-fingerprint-help = Uma impressão digital identifica um problema (eventos agrupados). Visível no URL de qualquer página de problema. Deixe em branco para corresponder a todos os problemas no âmbito.
alerts-unit-s = (s)
alerts-create-rule = Criar regra

# --- Agendamentos de resumo ---
alerts-digest-heading = Agendamentos de resumo
alerts-digest-desc = Resumos periódicos de atividade — balanços diários ou semanais em vez de ruído por evento.
alerts-digests-empty = Ainda não há agendamentos de resumo.
alerts-col-interval = Intervalo
alerts-col-last-sent = Último envio
alerts-col-enabled = Ativado
alerts-never = Nunca
alerts-yes = Sim
alerts-no = Não
alerts-digest-delete-confirm = Eliminar este agendamento de resumo?
alerts-add-digest = + Adicionar agendamento de resumo
alerts-interval-daily = Diário (24h)
alerts-interval-weekly = Semanal (7d)
alerts-interval-hourly = De hora a hora
alerts-create-schedule = Criar agendamento
