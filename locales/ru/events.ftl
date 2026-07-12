# Поверхность событий: межпроектный список событий и страница деталей события.
# event-detail-exception-stacktrace содержит инлайновый &amp; и рендерится с
# |safe. Счётные строки используют плюралы tv_count.

# --- Общие метки (список событий + детали события) ---
events-label-title = Заголовок
events-label-type = Тип
events-label-level = Уровень
events-label-platform = Платформа
events-label-environment = Окружение
events-label-time = Время
events-label-value = Значение

# --- Постраничная навигация (общая) ---
events-pagination-label = Постраничная навигация
events-pagination-prev = « Назад
events-pagination-next = Вперёд »

# --- Суффикс заголовка страницы (заголовки с динамическим префиксом) ---
events-title-suffix = — Stackpit

# --- Список событий ---
events-list-title = События — Stackpit
events-heading = События
events-list-search-placeholder = Поиск событий…
events-list-search-label = Поиск событий
events-list-select = Выбрать событие
events-list-filter-level = Фильтр по уровню
events-list-level-all = Все уровни
events-list-filter-type = Фильтр по типу
events-list-type-all = Все типы
events-list-project-placeholder = ID проекта
events-list-filter-project = Фильтр по проекту
events-list-filter-submit = Фильтровать
events-list-empty = Нет событий, соответствующих текущим фильтрам.
events-untitled = (без заголовка)
events-col-project = Проект

# --- Массовые действия ---
events-bulk-delete = Удалить
events-bulk-delete-selected-confirm = Удалить выбранные события?
events-bulk-delete-all = Удалить все { $count } совпадающих
events-bulk-delete-all-confirm = { $count ->
    [one] Безвозвратно удалить все { $count } совпадающее событие?
    [few] Безвозвратно удалить все { $count } совпадающих события?
    [many] Безвозвратно удалить все { $count } совпадающих событий?
   *[other] Безвозвратно удалить все { $count } совпадающих события?
}

# --- Количество (постраничная навигация) ---
events-count = { $count ->
    [one] { $count } событие
    [few] { $count } события
    [many] { $count } событий
   *[other] { $count } события
}

# --- Детали события ---
event-detail-event = Событие
event-detail-event-id-label = event_id:
event-detail-nav-label = Навигация по событиям
event-detail-nav-newer = « Новее
event-detail-nav-older = Старее »
event-detail-nav-count = { $count ->
    [one] { $count } событие
    [few] { $count } события
    [many] { $count } событий
   *[other] { $count } события
}
event-detail-nav-in-issue = в проблеме
event-detail-user-feedback = Отзыв пользователя
event-detail-anonymous = Аноним
event-detail-related-event = Связанное событие:
event-detail-exception-stacktrace = Исключение &amp; трассировка стека
event-detail-handled = обработано
event-detail-unhandled = необработано
event-detail-in = в
event-detail-var-name = Переменная
event-detail-no-source = Контекст исходного кода недоступен
event-detail-breadcrumbs = Хлебные крошки
event-detail-th-category = Категория
event-detail-th-message = Сообщение
event-detail-tags = Теги
event-detail-contexts = Контексты
event-detail-request = Запрос
event-detail-headers = Заголовки
event-detail-th-header = Заголовок
event-detail-query-string = Строка запроса
event-detail-body = Тело
event-detail-user-reports = Отчёты пользователей
event-detail-attachments = Вложения
event-detail-att-filename = Имя файла
event-detail-att-size = Размер
event-detail-download = Скачать
event-detail-web-vitals = Web Vitals
event-detail-raw-json = Сырой JSON
event-detail-props-heading = Свойства события
event-detail-prop-event-id = ID события
event-detail-prop-timestamp = Метка времени
event-detail-prop-transaction = Транзакция
event-detail-prop-release = Релиз
event-detail-prop-server = Сервер
event-detail-prop-sdk = SDK
event-detail-prop-received = Получено
event-detail-user-heading = Пользователь
event-detail-user-id = ID
event-detail-user-email = Эл. почта
event-detail-user-username = Имя пользователя
event-detail-user-ip = IP-адрес

# --- Отчёты клиента (исходы отброшенных событий) ---
# Использует events-untitled и events-pagination-* (общие, тот же файл).
client-reports-title = Отчёты клиента
client-reports-heading = Отчёты клиента
client-reports-dropped-heading = Отброшенные события
client-reports-dropped-subtitle = Что SDK отбросили перед отправкой, по категории и причине.
client-reports-th-category = Категория
client-reports-th-reason = Причина
client-reports-th-reasons = Причины
client-reports-th-dropped = Отброшено
client-reports-empty = Отчёты клиента для этого проекта не найдены.
client-reports-reports-heading = Отчёты
client-reports-delete = Удалить
client-reports-delete-selected-confirm = Удалить выбранные отчёты?
client-reports-th-event-id = ID события
client-reports-th-title = Заголовок
client-reports-th-timestamp = Метка времени
client-reports-th-platform = Платформа
client-reports-th-release = Релиз
client-reports-select = Выбрать отчёт
client-reports-delete-all = Удалить все { $count }
client-reports-delete-all-confirm = { $count ->
    [one] Удалить все { $count } совпадающий отчёт?
    [few] Удалить все { $count } совпадающих отчёта?
    [many] Удалить все { $count } совпадающих отчётов?
   *[other] Удалить все { $count } совпадающих отчёта?
}
client-reports-count = { $count ->
    [one] { $count } отчёт
    [few] { $count } отчёта
    [many] { $count } отчётов
   *[other] { $count } отчёта
}

# --- Отчёты пользователей (обратная связь пользователей) ---
# Использует events-untitled и events-pagination-* (общие, тот же файл).
user-reports-title = Отчёты пользователей
user-reports-heading = Отчёты пользователей
user-reports-empty = Отчёты пользователей для этого проекта не найдены.
user-reports-delete = Удалить
user-reports-delete-selected-confirm = Удалить выбранные отчёты?
user-reports-th-event-id = ID события
user-reports-th-title = Заголовок
user-reports-th-timestamp = Метка времени
user-reports-th-platform = Платформа
user-reports-th-release = Релиз
user-reports-select = Выбрать отчёт
user-reports-delete-all = Удалить все { $count }
user-reports-delete-all-confirm = { $count ->
    [one] Удалить все { $count } совпадающий отчёт?
    [few] Удалить все { $count } совпадающих отчёта?
    [many] Удалить все { $count } совпадающих отчётов?
   *[other] Удалить все { $count } совпадающих отчёта?
}
user-reports-count = { $count ->
    [one] { $count } отчёт
    [few] { $count } отчёта
    [many] { $count } отчётов
   *[other] { $count } отчёта
}
