# Русские соответствия locales/en/errors.ftl. Название бренда "Stackpit"
# остаётся в шаблонах дословно, как в base.html/login.html.
error-page-title = Ошибка - Stackpit
error-heading = Ошибка
error-not-found = Запрашиваемая страница не существует.
error-back-projects = Назад к проектам

# Страница подтверждения созданного приглашения (только английский/локаль по умолчанию).
invite-created-page-title = Приглашение создано - Stackpit
invite-created-heading = Приглашение создано
invite-created-share = Поделитесь этой ссылкой. Она действительна { $ttl } и одноразовая.
invite-created-back-members = Назад к участникам

# --- Flash-, успешные и валидационные сообщения (зависят от локали) ---

# Диагностика "не найдено". Префикс "Ошибка:" добавляется в Rust; значение
# несёт только сущность вместе с id.
flash-not-found-project = проект не найден: { $id }
flash-not-found-key = ключ API не найден: { $id }
flash-not-found-integration = интеграция не найдена: { $id }
flash-not-found-alert-rule = правило оповещения не найдено: { $id }
flash-not-found-digest-schedule = расписание сводок не найдено: { $id }
flash-not-found-repo = репозиторий не найден: { $id }
flash-not-found-project-integration = интеграция проекта не найдена: { $id }
flash-not-found-filter = { $label } не найден

# Валидация правил фильтра
flash-unrecognized-field = Неизвестное поле: { $value }
flash-unrecognized-operator = Неизвестный оператор: { $value }
flash-unrecognized-action = Неизвестное действие: { $value }

# Настройки проекта
flash-project-name-updated = Имя проекта обновлено
flash-project-name-too-long = Имя проекта превышает максимальную длину в { $max } символов
flash-repo-url-required = URL репозитория обязателен
flash-repo-url-too-long = URL репозитория превышает максимальную длину в 2048 символов
flash-repo-added = Репозиторий добавлен
flash-repo-removed = Репозиторий удалён
flash-project-archived = Проект архивирован
flash-project-unarchived = Проект разархивирован
flash-key-created = Ключ создан
flash-key-deleted = Ключ удалён

# Оповещения и сводки
flash-project-not-found-or-denied = Ошибка: проект не найден или доступ запрещён
flash-alert-rule-created = Правило оповещения создано
flash-alert-rule-deleted = Правило оповещения удалено
flash-digest-schedule-created = Расписание сводок создано
flash-digest-schedule-deleted = Расписание сводок удалено

# Интеграции проекта
flash-integration-not-found = Интеграция не найдена
flash-integration-activated = Интеграция активирована
flash-integration-updated = Интеграция обновлена
flash-integration-deactivated = Интеграция деактивирована

# Интеграции организации
flash-name-required = Имя обязательно
flash-invalid-integration-kind = Недопустимый тип интеграции
flash-invalid-email-provider = Недопустимый провайдер электронной почты
flash-api-token-required = Токен API обязателен.
flash-from-address-required = Адрес отправителя обязателен.
flash-smtp-not-configured = SMTP не настроен. Укажите [email] host в конфигурации сервера.
flash-invalid-to-address = Получатель должен быть корректным адресом электронной почты.
flash-test-digest-sent = Тестовая сводка поставлена в очередь для { $count } проект(ов) в их интеграции с включёнными сводками.
flash-test-digest-sample = Недавней активности нет, поэтому в очередь поставлена помеченная как образец сводка.
flash-test-digest-no-target = Ни в одной интеграции не включены сводки для проекта этого расписания.
flash-url-required = URL обязателен
flash-secret-not-configured = Не удаётся сохранить секрет: шифрование не настроено. Задайте STACKPIT_MASTER_KEY, чтобы включить хранение секретов.
flash-integration-license-required = Интеграции Slack, вебхуков и трекеров задач требуют активной коммерческой лицензии. Уведомления по электронной почте остаются доступными без лицензии.
flash-integration-created = Интеграция создана
flash-integration-name-exists = Интеграция с таким именем уже существует.
flash-integration-deleted = Интеграция удалена
flash-integration-no-url = Для интеграции не настроен URL
flash-test-notification-sent = Тестовое уведомление отправлено

# Входящие фильтры
flash-inbound-filters-updated = Входящие фильтры обновлены
flash-pattern-required = Шаблон обязателен
flash-message-filter-added = Фильтр сообщений добавлен
flash-message-filter-removed = Фильтр сообщений удалён
flash-rate-limit-updated = Ограничение частоты обновлено
flash-environment-required = Окружение обязательно
flash-environment-excluded = Окружение исключено
flash-environment-filter-removed = Фильтр окружения удалён
flash-release-filter-added = Фильтр релиза добавлен
flash-release-filter-removed = Фильтр релиза удалён
flash-ua-filter-added = Фильтр User-Agent добавлен
flash-ua-filter-removed = Фильтр User-Agent удалён
flash-rule-added = Правило добавлено
flash-rule-removed = Правило удалено
flash-cidr-required = CIDR обязателен
flash-invalid-cidr = Недопустимый формат CIDR
flash-ip-block-added = Блокировка IP добавлена
flash-ip-block-removed = Блокировка IP удалена

# Новый проект
flash-project-name-required = Имя проекта обязательно
flash-email-not-configured = Электронная почта не настроена. Добавьте в конфигурацию сервера секцию [email] с провайдером.
flash-integration-saved = Интеграция обновлена
flash-integration-global-not-for-trackers = Трекеры задач не используют доставку на всю организацию; репозиторий назначения берётся из настроек репозиториев каждого проекта.
flash-project-excluded = Проект исключён из этой интеграции
flash-project-included = Проект больше не исключён
flash-global-email-needs-recipient = Почтовой интеграции на всю организацию нужен получатель по умолчанию: у проектов, которые её не активировали, нет собственного адреса.
flash-queue-item-not-found = Уведомление в очереди не найдено
flash-queue-replayed = Уведомление доставлено и удалено из очереди
flash-queue-replay-failed = Повторная отправка не удалась: { $error }
flash-queue-cancelled = Уведомление из очереди отброшено
flash-queue-replay-failed-generic = Повторная отправка не удалась. Причина указана в самой записи очереди, в поле «Ошибка».
flash-license-activated = Лицензия активирована
flash-license-deactivated = Лицензия удалена
flash-license-persist-failed = Лицензия прошла проверку, но не сохранилась. Посмотрите журнал сервера.
flash-license-clear-failed = Не удалось удалить лицензию. Посмотрите журнал сервера.
flash-license-empty = Вставьте лицензионный ключ, чтобы активировать.
flash-license-bad-signature = Эта лицензия не подходит для этой установки. Проверьте, что вставили нужный ключ.
flash-license-wrong-product = Эта лицензия не для Stackpit.
flash-license-unreadable = Не удалось прочитать эту лицензию. Проверьте её и попробуйте снова.
