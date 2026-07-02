# Логи: список логов по проекту. Использует nav-logs. Строки со счётчиком
# используют плюрали tv_count ([one]/[other]).

# --- Суффикс заголовка страницы ---
logs-title-suffix = — Stackpit

# --- Список логов ---
logs-list-search-placeholder = Поиск по логам…
logs-list-search-label = Поиск по логам
logs-list-filter-level = Фильтр по уровню
logs-list-level-all = Все уровни
logs-filter-submit = Фильтровать
logs-list-empty = Нет логов, соответствующих текущим фильтрам.
logs-col-timestamp = Метка времени
logs-col-level = Уровень
logs-col-body = Сообщение
logs-col-trace = Трасса
logs-col-release = Релиз
logs-body-empty = (пусто)

# --- Постраничная навигация ---
logs-pagination-label = Постраничная навигация
logs-pagination-prev = « Назад
logs-pagination-next = Далее »
logs-count = { $count ->
    [one] { $count } лог
    [few] { $count } лога
    [many] { $count } логов
   *[other] { $count } лога
}
