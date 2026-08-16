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

# Трекер задач
integrations-add-tracker = + Трекер задач
integrations-tracker-title = Добавить трекер задач — Stackpit
integrations-tracker-breadcrumb = Добавить трекер задач
integrations-tracker-heading = Добавить интеграцию с трекером задач
integrations-tracker-kind-label = Трекер
integrations-tracker-name-placeholder = например, GitHub Issues
integrations-tracker-url-label = Базовый URL
integrations-tracker-token-label = API-токен
integrations-tracker-token-placeholder = Персональный токен доступа
integrations-tracker-target-help = Репозиторий назначения берётся из настроек репозиториев каждого проекта, поэтому здесь он не настраивается. Добавьте репозиторий в настройках проекта.
integrations-global-label = Доставлять во все проекты
integrations-global-help = Оповещения идут во все проекты этой организации, кроме тех, которые вы исключите на странице этой интеграции. Фильтры уровня и окружения на уровне проекта продолжают действовать поверх.
integrations-global-badge = вся организация
integrations-global-save = Сохранить доставку
integrations-global-on = Доставлять во всю организацию
integrations-global-off = Прекратить доставку во всю организацию

# Детали интеграции: доставка по проектам
integrations-detail-title = Интеграция — Stackpit
integrations-back = Назад к интеграциям
integrations-projects-heading = Доставка по проектам
integrations-projects-hint-global = Эта интеграция доставляет во все проекты ниже, если вы их не исключите. Исключение — единственный способ отказаться; списка включения нет.
integrations-projects-hint-per-project = Эта интеграция доставляет только туда, где проект её активировал. Отметьте её как общую для организации, чтобы доставлять везде.
integrations-projects-hint-tracker = Трекеры задач сопоставляются с репозиториями проекта по типу форджа и хосту. Исключение проекта убирает этот трекер из вариантов создания задачи.
integrations-projects-empty = В этой организации пока нет проектов.
integrations-col-project = Проект
integrations-col-state = Состояние
integrations-project-archived = в архиве
integrations-state-default = Доставляется
integrations-state-customised = Настроено
integrations-state-excluded = Исключено
integrations-state-no-repo = Нет подходящего репозитория
integrations-state-not-routed = Не активировано
integrations-exclude = Исключить
integrations-include = Включить
integrations-email-to-label = Получатель по умолчанию
integrations-email-to-help = Используется там, где проект не задал собственный адрес. Обязателен для интеграции на всю организацию.
