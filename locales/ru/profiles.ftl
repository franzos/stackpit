# Интерфейс профилей: список профилей проекта и страница деталей профиля.
# Повторно использует nav-profiles. Строки со счётчиками используют плюрали
# tv_count ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы ---
profiles-title-suffix = — Stackpit

# --- Список профилей ---
profiles-list-empty = Профили не найдены. События профилей с <code class="text-mono">item_type = "profile"</code> появятся здесь.
profiles-col-event-id = ID события
profiles-col-transaction = Транзакция
profiles-col-platform = Платформа
profiles-col-release = Релиз
profiles-col-environment = Окружение
profiles-col-timestamp = Метка времени

# --- Детали профиля ---
profiles-detail-heading = Профиль
profiles-detail-raw-payload = Необработанные данные

# --- Постраничная навигация ---
profiles-pagination-label = Постраничная навигация
profiles-pagination-prev = « Назад
profiles-pagination-next = Вперёд »
profiles-count = { $count ->
    [one] { $count } профиль
    [few] { $count } профиля
    [many] { $count } профилей
   *[other] { $count } профиля
}
