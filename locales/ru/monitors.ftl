# Интерфейс мониторов: список мониторов проекта (cron-чекины) и страница
# деталей монитора. Повторно использует nav-monitors. Строки со счётчиками
# используют плюрали tv_count ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы ---
monitors-title-suffix = — Stackpit

# --- Список мониторов ---
monitors-list-empty = Мониторы не найдены. События чекинов с <code class="text-mono">monitor_slug</code> появятся здесь.
monitors-col-slug = Slug
monitors-col-last-status = Последний статус
monitors-col-last-checkin = Последний чекин
monitors-col-count = Количество

# --- Детали монитора ---
monitors-detail-title-prefix = Монитор
monitors-detail-subtitle = Чекины монитора.
monitors-detail-empty = Для этого монитора чекины не найдены.
monitors-detail-select-checkin = Выбрать чекин
monitors-detail-confirm-delete-selected = Удалить выбранные чекины?
monitors-detail-delete = Удалить
monitors-detail-col-title = Заголовок
monitors-detail-col-level = Уровень
monitors-detail-col-environment = Окружение
monitors-detail-col-time = Время
monitors-detail-untitled = (без названия)
monitors-detail-confirm-delete-all = { $count ->
    [one] Удалить все { $count } чекин?
    [few] Удалить все { $count } чекина?
    [many] Удалить все { $count } чекинов?
   *[other] Удалить все { $count } чекина?
}
monitors-detail-delete-all = { $count ->
    [one] Удалить все { $count }
    [few] Удалить все { $count }
    [many] Удалить все { $count }
   *[other] Удалить все { $count }
}

# --- Постраничная навигация ---
monitors-pagination-label = Постраничная навигация
monitors-pagination-prev = « Назад
monitors-pagination-next = Вперёд »
monitors-detail-count = { $count ->
    [one] { $count } чекин
    [few] { $count } чекина
    [many] { $count } чекинов
   *[other] { $count } чекина
}
