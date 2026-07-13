# Интерфейс проектов: список, создание, настройки (общие/ключи/sourcemaps/
# фильтры), интеграции и подтверждение создания. Значения, выводимые с |safe,
# содержат встроенную HTML-разметку; теги остаются идентичными, переведён
# только текст.

# --- Список проектов ---
projects-list-title = Проекты — Stackpit
projects-list-heading = Проекты
projects-list-subtitle = Следите за состоянием всей вашей архитектуры.
projects-list-all-events = Все события
projects-list-all-releases = Все релизы
projects-list-new = + Новый проект
projects-list-search-placeholder = Поиск проектов по названию, платформе или владельцу…
projects-list-search-label = Поиск проектов
projects-list-filter = Фильтр
projects-list-empty = Проекты не найдены. События появятся здесь, как только поступят.
projects-period-label = Период времени
projects-period-all = За всё время
projects-period-1h = Последний час
projects-period-24h = Последние 24 часа
projects-period-7d = Последние 7 дней
projects-period-14d = Последние 14 дней
projects-period-30d = Последние 30 дней
projects-period-90d = Последние 90 дней
projects-period-365d = Последние 365 дней
projects-col-project = Проект
projects-col-platforms = Платформы
projects-col-issues = Ошибки
projects-col-events = События
projects-col-breakdown = Разбивка
projects-col-release = Релиз
projects-col-first-seen = Впервые замечено
projects-col-last-seen = Последний раз замечено
projects-breakdown-errors = Ошибки:
projects-breakdown-transactions = Транзакции:
projects-breakdown-sessions = Сессии:
projects-breakdown-other = Прочее:
projects-legend-errors = Ошибки
projects-legend-transactions = Транзакции
projects-legend-sessions = Сессии
projects-legend-other = Прочее

# --- Общее для форм проекта ---
projects-optional = (необязательно)
projects-cancel = Отмена
projects-remove = Удалить
projects-delete = Удалить
projects-name-placeholder = Мой проект

# --- Новый проект ---
projects-new-title = Новый проект — Stackpit
projects-new-heading = Новый проект
projects-new-name-label = Название проекта
projects-new-platform-label = Платформа
projects-new-platform-select = Выберите платформу…
projects-new-platform-other = Другая
projects-new-platform-native = Native (C/C++)
projects-new-submit = Создать проект

# --- Вкладки настроек (общие для страниц настроек) ---
projects-tab-general = Общие
projects-tab-sdk = Настройка SDK
projects-tab-sourcemaps = Source maps
projects-tab-filters = Фильтры
projects-tab-integrations = Интеграции

# --- Настройки: общие ---
projects-settings-heading = Настройки
projects-settings-archived = (в архиве)
projects-settings-name-heading = Название проекта
projects-settings-display-name = Отображаемое имя
projects-settings-save-name = Сохранить название
projects-settings-info-heading = Информация о проекте
projects-settings-status = Статус
projects-settings-source = Источник
projects-repos-heading = Репозитории исходного кода
projects-repos-help = Связывайте кадры стека с исходным кодом в вашей forge. Зарегистрируйте релиз с SHA коммита через <code class="text-mono">sentry-cli</code>, чтобы активировать ссылки.
projects-repos-empty = Репозитории не настроены.
projects-repos-url-label = URL репозитория
projects-repos-col-forge = Forge
projects-repos-template = Шаблон URL
projects-repos-auto = авто
projects-repos-remove-confirm = Удалить этот репозиторий?
projects-repos-add = Добавить репозиторий
projects-repos-add-help = Добавляет кликабельные ссылки на исходный код (например, «Открыть на GitHub») рядом с кадрами стека. Требуется релиз с SHA коммита — тип forge определяется автоматически. Поддерживаются: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. Для других forge укажите шаблон URL.
projects-danger-heading = Опасная зона
projects-archive-desc = Архивировать этот проект. Архивные проекты отклоняют новые события.
projects-archive-confirm = Архивировать этот проект? Новые события будут отклоняться.
projects-archive-submit = Архивировать проект
projects-unarchive-desc = Разархивировать этот проект, чтобы снова принимать события.
projects-unarchive-submit = Разархивировать проект
projects-delete-desc = Безвозвратно удалить этот проект и все его данные. Это действие необратимо.
projects-delete-confirm = Удалить этот проект и ВСЕ его данные? Это действие необратимо.
projects-delete-submit = Удалить проект
projects-move-heading = Переместить в организацию
projects-move-desc = Переместите этот проект в другую организацию, владельцем которой вы являетесь. Данные и DSN остаются действительными, но интеграции уведомлений отвязываются и должны быть добавлены заново в новой организации.
projects-move-target-label = Целевая организация
projects-move-confirm-pre = Введите
projects-move-confirm-post = для подтверждения.
projects-move-confirm-placeholder = Название проекта
projects-move-confirm-dialog = Переместить этот проект в выбранную организацию?
projects-move-submit = Переместить проект
projects-move-err-invalid-target = Недопустимая целевая организация.
projects-move-err-name-mismatch = Название проекта не совпадает.
projects-move-err-denied = Вы не являетесь владельцем целевой организации.
projects-move-err-conflict = Не удалось переместить проект; возможно, он изменился. Повторите попытку.

# --- Настройки: настройка SDK / ключи ---
projects-keys-title = Настройка SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = Ключи не зарегистрированы. Создайте ключ ниже, чтобы получить DSN.
projects-keys-list-heading = Ключи проекта
projects-keys-empty = Для этого проекта нет зарегистрированных ключей.
projects-keys-col-public = Публичный ключ
projects-keys-col-label = Метка
projects-keys-col-status = Статус
projects-keys-col-created = Создан
projects-keys-delete-confirm = Удалить этот ключ? SDK, использующие его, перестанут работать.
projects-keys-create-heading = Создать ключ
projects-keys-label-label = Метка
projects-keys-label-placeholder = напр. production, staging
projects-keys-create-submit = Создать ключ

# --- Настройки: source maps ---
projects-sourcemaps-title = Source Maps
projects-sourcemaps-apikey-heading = API-ключ
projects-sourcemaps-apikey-desc = Для загрузки source maps требуется API-ключ. Действует только для этого проекта и только для операций с source maps.
projects-sourcemaps-key-generated = Ключ сгенерирован:
projects-sourcemaps-key-warning = Скопируйте этот ключ сейчас — он больше не будет показан.
projects-sourcemaps-col-key = Ключ
projects-sourcemaps-regen-confirm = Сгенерировать ключ заново? Текущий ключ перестанет работать.
projects-sourcemaps-regen = Сгенерировать заново
projects-sourcemaps-empty = Нет API-ключа source maps для этого проекта.
projects-sourcemaps-generate = Сгенерировать ключ
projects-sourcemaps-setup-heading = Настройка
projects-sourcemaps-setup-desc = Используйте <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> для загрузки source maps. Установите эти переменные окружения:
projects-sourcemaps-then-upload = Затем загрузите:

# --- Настройки: фильтры ---
projects-filters-inbound-heading = Входящие фильтры
projects-filters-inbound-desc = Встроенные фильтры, отбрасывающие события, соответствующие распространённым шумовым шаблонам.
projects-filters-browser-ext = Расширения браузера — отбрасывать события от расширений Chrome/Firefox/Safari
projects-filters-localhost = Localhost — отбрасывать события с localhost, 127.0.0.1, приватных IP
projects-filters-inbound-submit = Сохранить входящие фильтры
projects-filters-message-heading = Фильтры сообщений
projects-filters-message-help = Glob-шаблоны, сопоставляемые с заголовками событий. Используйте <code class="text-mono">*</code> для любой последовательности, <code class="text-mono">?</code> для одного символа.
projects-filters-col-pattern = Шаблон
projects-filters-message-empty = Фильтры сообщений не настроены.
projects-filters-add-pattern = Добавить шаблон
projects-filters-message-submit = Добавить фильтр сообщений
projects-filters-ratelimit-heading = Ограничение частоты
projects-filters-ratelimit-desc = Максимум событий в минуту для этого проекта. 0 = без ограничений.
projects-filters-ratelimit-label = Событий в минуту
projects-filters-ratelimit-submit = Сохранить ограничение частоты
projects-filters-env-heading = Исключённые окружения
projects-filters-env-desc = События из этих окружений будут молча отбрасываться.
projects-filters-col-environment = Окружение
projects-filters-env-empty = Нет исключённых окружений.
projects-filters-env-add-label = Добавить исключённое окружение
projects-filters-env-submit = Исключить окружение
projects-filters-release-heading = Фильтры релизов
projects-filters-release-desc = Glob-шаблоны, сопоставляемые с версиями релизов. Совпадающие события отбрасываются.
projects-filters-release-empty = Нет фильтров релизов.
projects-filters-release-submit = Добавить фильтр релизов
projects-filters-ua-heading = Фильтры User-Agent
projects-filters-ua-desc = Glob-шаблоны, сопоставляемые с заголовками User-Agent. Встроенные шаблоны для kube-probe и проверок состояния всегда активны.
projects-filters-ua-empty = Нет пользовательских фильтров User-Agent.
projects-filters-ua-submit = Добавить фильтр User-Agent
projects-filters-rules-heading = Пользовательские правила
projects-filters-rules-desc = Расширенные правила, сопоставляющие поля событий. Правила с более высоким приоритетом обрабатываются первыми.
projects-filters-col-field = Поле
projects-filters-col-operator = Оператор
projects-filters-col-value = Значение
projects-filters-col-action = Действие
projects-filters-col-priority = Приоритет
projects-filters-rules-empty = Нет пользовательских правил.
projects-filters-sample-rate-label = Частота выборки
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Добавить правило
projects-filters-op = { $op ->
    [not_equals] не равно
    [contains] содержит
    [not_contains] не содержит
    [starts_with] начинается с
    [in] в списке
    [not_in] не в списке
   *[equals] равно
}
projects-filters-action = { $action ->
    [sample] выборка
   *[drop] отбросить
}
projects-filters-ip-heading = Список блокировки IP
projects-filters-ip-desc = CIDR-блоки или отдельные IP. События с заблокированных IP молча отбрасываются.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = IP-блоки не настроены.
projects-filters-ip-add-label = Добавить CIDR
projects-filters-ip-submit = Заблокировать диапазон IP
projects-filters-discard-heading = Статистика отбрасывания
projects-filters-discard-window = (последние 7 дней)
projects-filters-col-date = Дата
projects-filters-col-reason = Причина
projects-filters-col-count = Количество

# Метки сущностей фильтров, подставляемые в flash-not-found-filter при удалении.
projects-filter-label-message = фильтр сообщений
projects-filter-label-environment = фильтр окружений
projects-filter-label-release = фильтр релизов
projects-filter-label-user-agent = фильтр User-Agent
projects-filter-label-rule = правило фильтра

# --- Настройки: интеграции ---
projects-integrations-active-heading = Активные интеграции
projects-integrations-active-empty = Нет активированных интеграций. Сначала добавьте глобальную интеграцию на странице <a class="text-primary" href="/web/settings/integrations/">Интеграции</a>, затем включите её здесь. Каждую можно ограничить минимальным уровнем и окружением, чтобы dev-шум не попадал в prod-каналы.
projects-integrations-deactivate-confirm = Деактивировать эту интеграцию для проекта?
projects-integrations-deactivate = Деактивировать
projects-integrations-notify-new-issues = Новые ошибки
projects-integrations-notify-regressions = Регрессии
projects-integrations-notify-threshold = Пороговые оповещения
projects-integrations-notify-digests = Сводки
projects-integrations-min-level = Минимальный уровень
projects-integrations-level-any = Любой
projects-integrations-env-filter = Фильтр окружения
projects-integrations-env-placeholder = напр. production
projects-integrations-to-address = Адрес получателя
projects-integrations-to-address-note = (только для email-интеграций)
projects-integrations-activate-heading = Активировать интеграцию
projects-integrations-integration-label = Интеграция
projects-integrations-activate-submit = Активировать
projects-integrations-available-empty = Нет доступных интеграций. <a class="text-primary" href="/web/settings/integrations/">Сначала создайте одну</a>.

# --- Проект создан ---
projects-created-word = создан
projects-created-breadcrumb = Создан
projects-created-heading = Проект создан
projects-created-subtitle = Используйте DSN ниже для настройки вашего SDK.
projects-created-settings-btn = Настройки проекта
projects-created-back = Назад к проектам
projects-created-details-heading = Детали проекта
projects-created-col-id = ID проекта
projects-created-sdk-desc-before = Установите Sentry SDK для
projects-created-sdk-desc-after = и инициализируйте его с указанным выше DSN.
projects-created-docs-javascript = Документация Sentry JavaScript →
projects-created-docs-python = Документация Sentry Python →
projects-created-docs-rust = Документация Sentry Rust →
projects-created-docs-go = Документация Sentry Go →
projects-created-docs-node = Документация Sentry Node.js →
projects-created-docs-java = Документация Sentry Java →
projects-created-docs-ruby = Документация Sentry Ruby →
projects-created-docs-php = Документация Sentry PHP →
projects-created-docs-elixir = Документация Sentry Elixir →
projects-created-docs-dotnet = Документация Sentry .NET →
projects-created-docs-apple = Документация Sentry Apple →
projects-created-docs-kotlin = Документация Sentry Kotlin →
projects-created-docs-native = Документация Sentry Native →
projects-created-docs-generic = Документация платформы Sentry →
