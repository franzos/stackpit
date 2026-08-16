# Раздел настроек: страница значений по умолчанию для браузера
# (templates/browser_defaults.html, ключи defaults-*) и отдельная страница
# провижининга организаций (templates/provision.html, ключи provision-*).
# Повторно использует nav-settings. Значения уровней (fatal/error/...) остаются
# без перевода в шаблоне, как на разделах ошибок/событий, где уровни лога
# сохраняются в каноническом английском виде.

# --- Значения по умолчанию для браузера ---
defaults-page-title = Значения по умолчанию (браузер) — Stackpit
defaults-subtitle = Задайте значения фильтров по умолчанию для страниц-списков. Хранятся как cookie браузера.
defaults-none = Без значения по умолчанию
defaults-status-label = Статус по умолчанию (ошибки)
defaults-status-unresolved = Не решено
defaults-status-resolved = Решено
defaults-status-ignored = Проигнорировано
defaults-level-label = Уровень по умолчанию
defaults-period-label = Диапазон времени по умолчанию
defaults-save = Сохранить значения по умолчанию
defaults-clear-confirm = Очистить все значения по умолчанию для браузера?
defaults-clear = Очистить все значения по умолчанию
flash-defaults-saved = Значения по умолчанию сохранены
flash-defaults-cleared = Значения по умолчанию очищены

# --- Предпочитаемый язык ---
settings-language-heading = Предпочитаемый язык
settings-language-subtitle = Выберите язык интерфейса Stackpit. Для вошедших в систему аккаунтов настройка сохраняется на всех устройствах.
settings-language-label = Язык
settings-language-save = Сохранить язык

settings-aria-sections = Разделы настроек

# --- Страница провижининга (отдельная страница) ---
provision-page-title = Настройка организаций — Stackpit
provision-heading = Настройка организаций
provision-subtitle-1 = Следующие организации доступны от вашего провайдера идентификации.
provision-subtitle-2 = Выберите те, которые хотите создать в Stackpit.
provision-create = Создать выбранные
provision-skip = Пропустить

# Очередь доставки
queue-page-title = Очередь доставки — Stackpit
queue-subtitle = Уведомления, которые не удалось доставить. Они повторяются автоматически в течение 24 часов, а затем ждут вас здесь.
queue-count-pending = { $count } в ожидании
queue-count-failed = { $count } с ошибкой
queue-empty = Очередь пуста. Все уведомления доставлены.
queue-col-integration = Интеграция
queue-col-project = Проект
queue-col-state = Состояние
queue-col-attempts = Попытки
queue-col-queued = В очереди с
queue-col-error = Последняя ошибка
queue-state-pending = Повтор
queue-state-failed = Отказ
queue-replay = Отправить снова
queue-cancel = Отбросить
queue-cancel-confirm = Отбросить это уведомление, не доставляя его?
