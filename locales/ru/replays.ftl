# Раздел реплеев: список реплеев по проекту и страница детали реплея.
# Повторно использует nav-replays. Счётные строки используют плюрали tv_count
# ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы ---
replays-title-suffix = — Stackpit

# --- Список реплеев ---
replays-list-empty = Реплеи не найдены. События реплеев появятся здесь.
replays-col-event-id = ID события
replays-col-type = Тип
replays-col-release = Релиз
replays-col-url = URL
replays-col-user = Пользователь
replays-col-browser = Браузер
replays-col-duration = Длительность
replays-col-errors = Ошибки
replays-col-environment = Окружение
replays-col-timestamp = Метка времени

# --- Деталь реплея ---
replays-detail-heading = Реплей
replays-detail-note = Воспроизведение записи пока недоступно. Ниже показаны исходные данные реплея.
replays-detail-raw-payload = Исходные данные
replays-related-errors = Ошибки в этом реплее
replays-col-level = Уровень
replays-col-title = Заголовок

# --- Постраничная навигация ---
replays-pagination-label = Постраничная навигация
replays-pagination-prev = « Назад
replays-pagination-next = Вперёд »
replays-count = { $count ->
    [one] { $count } реплей
    [few] { $count } реплея
    [many] { $count } реплеев
   *[other] { $count } реплея
}
