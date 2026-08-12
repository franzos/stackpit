# Интеграции: список (templates/integrations.html) и три формы «добавления»
# (webhook, Slack, e-mail). Используют nav-settings/nav-integrations для
# оформления. Разделительные пробелы находятся в шаблоне. integrations-empty
# содержит встроенную разметку <strong> и символ стрелки, рендерится с |safe.
integrations-page-title = Интеграции — Stackpit
integrations-subtitle = Отправка через webhook, Slack и e-mail. Маршрутизация по проектам настраивается в параметрах каждого проекта.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + E-mail
integrations-license-required-badge = Нужна лицензия
integrations-empty = Пока нет интеграций. Добавьте одну выше, чтобы начать получать уведомления. После добавления включите её для каждого проекта в разделе <strong>Настройки проекта → Интеграции</strong>.
integrations-col-name = Название
integrations-col-type = Тип
integrations-col-endpoint = Эндпоинт
integrations-col-created = Создано
integrations-delete-confirm = Удалить эту интеграцию? Она будет удалена из всех проектов.
integrations-test = Проверить
integrations-delete = Удалить
flash-test-failed = Проверка не удалась: { $error }

# Общие подписи полей и кнопки для трёх форм добавления интеграции.
integrations-cancel = Отмена
integrations-optional = (необязательно)
integrations-required = (обязательно)
integrations-create = Создать интеграцию

# --- Добавить webhook ---
integrations-webhook-title = Добавить webhook — Stackpit
integrations-webhook-breadcrumb = Добавить webhook
integrations-webhook-heading = Добавить интеграцию webhook
integrations-webhook-name-placeholder = напр. Оповещения продакшена
integrations-webhook-url-label = URL webhook
integrations-webhook-secret-label = Секрет HMAC
integrations-webhook-secret-placeholder = Необязательный секрет для подписи

# --- Добавить Slack ---
integrations-slack-title = Добавить Slack — Stackpit
integrations-slack-breadcrumb = Добавить Slack
integrations-slack-heading = Добавить интеграцию Slack
integrations-slack-name-placeholder = напр. канал #alerts
integrations-slack-url-label = URL webhook Slack

# --- Добавить e-mail ---
integrations-email-title = Добавить e-mail — Stackpit
integrations-email-breadcrumb = Добавить e-mail
integrations-email-heading = Добавить интеграцию e-mail
integrations-email-name-placeholder = напр. Оповещения команды по e-mail
integrations-email-lock-pre = Провайдер и отправитель берутся из
integrations-email-lock-post = конфигурации сервера; эта интеграция задаёт только получателя.
integrations-email-provider-label = Провайдер
integrations-email-token-label = API-токен
integrations-email-token-placeholder-default = Оставьте пустым, чтобы использовать значение по умолчанию
integrations-email-token-placeholder = API-токен провайдера
integrations-email-from-label = Адрес отправителя
integrations-email-fromname-label = Имя отправителя
integrations-email-smtp-hint = SMTP использует подключение [email] сервера; отдельный токен для интеграции не нужен.
