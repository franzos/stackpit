# Раздел транзакций: список транзакций по проекту и страница детали транзакции
# (экземпляры). Повторно использует nav-transactions для заголовка/хлебных
# крошек/названия. Счётные строки используют плюрали tv_count
# ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы (заголовки с динамическим префиксом) ---
transactions-title-suffix = — Stackpit

# --- Список транзакций ---
transactions-time-range = Диапазон времени
transactions-period-1h = Последний час
transactions-period-24h = Последние 24 ч
transactions-period-7d = Последние 7 дней
transactions-period-14d = Последние 14 дней
transactions-period-30d = Последние 30 дней
transactions-period-90d = Последние 90 дней
transactions-filter-submit = Фильтровать
transactions-list-empty = Нет транзакций за этот период.
transactions-col-name = Транзакция
transactions-col-throughput = Пропускная способность
transactions-col-failure = % сбоев
transactions-col-count = Количество
transactions-col-users = Пользователи

# --- Деталь транзакции (экземпляры) ---
transactions-detail-op = op:
transactions-detail-empty = Для этой транзакции не записано ни одного экземпляра.
transactions-detail-col-duration = Длительность
transactions-detail-col-status = Статус
transactions-detail-col-trace = Трейс
transactions-detail-col-when = Когда
transactions-detail-distribution = Распределение длительности

# --- Постраничная навигация (деталь транзакции) ---
transactions-pagination-label = Постраничная навигация
transactions-pagination-prev = « Назад
transactions-pagination-next = Вперёд »
transactions-detail-count = { $count ->
    [one] { $count } экземпляр
    [few] { $count } экземпляра
    [many] { $count } экземпляров
   *[other] { $count } экземпляра
}
