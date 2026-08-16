# Ошибки: список ошибок, сгруппированных по отпечатку, и страница деталей
# ошибки. issue-detail-exception-stacktrace содержит встроенный &amp; и
# рендерится с |safe. Строки со счётчиком используют плюрали tv_count.

# --- Общие подписи (список ошибок + детали ошибки) ---
issues-label-title = Заголовок
issues-label-level = Уровень
issues-label-events = События
issues-label-users = Пользователи
issues-label-trend = Тренд
issues-trend-tooltip = Объём событий за выбранный период
issues-label-status = Статус
issues-label-first-seen = Впервые замечено
issues-label-last-seen = Последний раз замечено
issues-label-value = Значение

# --- Значения статуса (варианты фильтра + бейджи) ---
issues-status-unresolved = Не решено
issues-status-resolved = Решено
issues-status-ignored = Игнорируется

# --- Постраничная навигация (общая) ---
issues-pagination-label = Постраничная навигация
issues-pagination-prev = « Назад
issues-pagination-next = Далее »

# --- Суффикс заголовка страницы (заголовки с динамическим префиксом) ---
issues-title-suffix = — Stackpit

# --- Список ошибок ---
issues-list-subtitle = Ошибки, сгруппированные по отпечатку.
issues-list-filtered-by-tag = Фильтр по тегу:
issues-list-clear-tag = Сбросить фильтр по тегу
issues-list-search-placeholder = Поиск по ошибкам…
issues-list-search-label = Поиск по ошибкам
issues-list-select = Выбрать ошибку
issues-list-filter-status = Фильтр по статусу
issues-list-status-all = Все статусы
issues-list-filter-level = Фильтр по уровню
issues-list-level-all = Все уровни
issues-list-filter-release = Фильтр по релизу
issues-list-release-all = Все релизы
issues-list-filter-environment = Фильтр по окружению
issues-list-environment-all = Все окружения
issues-period-label = Период времени
issues-list-filter-submit = Фильтровать
issues-list-empty = Нет ошибок, соответствующих текущим фильтрам.
issues-untitled = (без названия)

# --- Массовые действия ---
issues-bulk-resolve-all = Решить все { $count }
issues-bulk-ignore-all = Игнорировать все { $count }
issues-bulk-delete-all = Удалить все { $count }
issues-bulk-resolve-confirm = { $count ->
    [one] Решить { $count } соответствующую ошибку?
    [few] Решить все { $count } соответствующие ошибки?
    [many] Решить все { $count } соответствующих ошибок?
   *[other] Решить все { $count } соответствующие ошибки?
}
issues-bulk-ignore-confirm = { $count ->
    [one] Игнорировать { $count } соответствующую ошибку?
    [few] Игнорировать все { $count } соответствующие ошибки?
    [many] Игнорировать все { $count } соответствующих ошибок?
   *[other] Игнорировать все { $count } соответствующие ошибки?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] Навсегда удалить { $count } соответствующую ошибку?
    [few] Навсегда удалить все { $count } соответствующие ошибки?
    [many] Навсегда удалить все { $count } соответствующих ошибок?
   *[other] Навсегда удалить все { $count } соответствующие ошибки?
}
issues-bulk-resolve = Решить
issues-bulk-ignore = Игнорировать
issues-bulk-delete = Удалить
issues-bulk-delete-selected-confirm = Навсегда удалить выбранные ошибки?

# --- Количество (постраничная навигация) ---
issues-count = { $count ->
    [one] { $count } ошибка
    [few] { $count } ошибки
    [many] { $count } ошибок
   *[other] { $count } ошибки
}

# --- Детали ошибки ---
issue-detail-title-fallback = Ошибка
issue-detail-resolve = ✓ Решить
issue-detail-reopen = Открыть заново
issue-detail-unignore = Не игнорировать
issue-detail-tab-details = Подробности
issue-detail-tab-events = Все события
issue-detail-exception-stacktrace = Исключение &amp; стек вызовов
issue-detail-handled = обработано
issue-detail-unhandled = не обработано
issue-detail-in = в
issue-detail-var-name = Переменная
issue-detail-no-source = Контекст исходного кода недоступен
issue-detail-in-app-only = Только кадры приложения
issue-detail-reverse-order = Обратный порядок
issue-detail-copy = Копировать
issue-detail-copy-frame = Скопировать этот кадр
issue-detail-library-frames = { $count ->
    [one] { $count } библиотечный кадр
    [few] { $count } библиотечных кадра
    [many] { $count } библиотечных кадров
   *[other] { $count } библиотечных кадра
}
issue-detail-minified-hint = Эти фреймы выглядят минифицированными, source map не применена.
issue-detail-minified-hint-link = Загрузить source maps
issue-detail-breadcrumbs = Хлебные крошки
issue-detail-th-time = Время
issue-detail-th-category = Категория
issue-detail-th-message = Сообщение
issue-detail-crumb-data = данные
issue-detail-crumb-filter = Фильтр хлебных крошек по типу
issue-detail-crumb-filter-all = Все типы
issue-detail-tags = Теги
issue-detail-contexts = Контексты
issue-detail-additional-data = Дополнительные данные
issue-detail-view-replay = Открыть реплей
issue-detail-view-trace = Открыть трейс
issue-detail-request = Запрос
issue-detail-headers = Заголовки
issue-detail-th-header = Заголовок
issue-detail-query-string = Строка запроса
issue-detail-body = Тело
issue-detail-environment = Окружение
issue-detail-user-reports = Отчёты пользователей
issue-detail-anonymous = Аноним
issue-detail-attachments = Вложения
issue-detail-att-filename = Имя файла
issue-detail-att-type = Тип
issue-detail-att-size = Размер
issue-detail-download = Скачать
issue-detail-raw-json = Необработанный JSON
issue-detail-no-events = Для этой ошибки не найдено событий.
issue-detail-ev-id = ID события
issue-detail-ev-timestamp = Метка времени
issue-detail-ev-platform = Платформа
issue-detail-events-count = { $count ->
    [one] { $count } событие
    [few] { $count } события
    [many] { $count } событий
   *[other] { $count } события
}
issue-detail-props-heading = Свойства ошибки
issue-detail-fingerprint = Отпечаток
issue-detail-tag-facets = Фасеты тегов
issue-detail-discard-undo-title = Снова принимать будущие события с этим отпечатком
issue-detail-discard-undo = Отменить отклонение
issue-detail-discard-confirm = Отклонять все будущие события с этим отпечатком?
issue-detail-discard-title = Молча отбрасывать будущие события с этим отпечатком
issue-detail-discard = Отклонять будущие события
issue-detail-create-external-issue = Создать задачу
issue-detail-external-tracker = Внешний трекер
issue-detail-view-on = Открыть в
flash-tracker-create-failed = Не удалось создать задачу в трекере. Проверьте токен и репозиторий интеграции и попробуйте снова.
flash-tracker-config-incomplete = У этой интеграции с трекером не хватает репозитория или токена. Исправьте это в настройках интеграции.
issue-detail-external-unlink = Отвязать
issue-detail-external-unlink-confirm = Удалить эту связь? Задача останется на фордже — закройте или удалите её там.
issue-detail-external-orphaned = интеграция удалена
flash-tracker-unlinked = Связь удалена. Задача по-прежнему существует на фордже.
flash-tracker-ambiguous = У этого проекта несколько репозиториев, в которые может писать этот трекер. Выберите один и попробуйте снова.
