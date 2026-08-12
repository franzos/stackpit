# Раздел транзакций: список транзакций по проекту и страница детали транзакции
# (экземпляры). Повторно использует nav-transactions для заголовка/хлебных
# крошек/названия. Счётные строки используют плюрали tv_count
# ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы (заголовки с динамическим префиксом) ---
transactions-title-suffix = — Stackpit

# --- Список транзакций ---
transactions-time-range = Диапазон времени
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
transactions-detail-spans = Разбивка по спанам
transactions-detail-issues = Связанные проблемы
transactions-detail-instances = Самые медленные экземпляры
transactions-detail-trend = Динамика перцентилей
transactions-detail-trend-note = Отмечены точки, где p95 превысил медиану пяти предыдущих точек более чем в 1,5 раза.

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
