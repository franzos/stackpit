# Раздел спанов: список спанов/трейсов по проекту (spans-*) и страница детали
# трейса в виде водопада (trace-detail-*). Повторно использует nav-spans.
# Счётные строки используют плюрали tv_count ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы ---
spans-title-suffix = — Stackpit

# --- Список спанов/трейсов ---
spans-list-empty = Спаны для этого проекта не найдены.
spans-traces-heading = Трейсы
spans-all-heading = Все спаны

# --- Таблица трейсов ---
spans-col-trace-id = ID трейса
spans-col-root-op = Корневая операция
spans-col-root-description = Корневое описание
spans-col-duration = Длительность
spans-col-first-seen = Первое появление
spans-col-last-seen = Последнее появление

# --- Таблица агрегированных спанов (по операции/описанию) ---
spans-agg-heading = Операции спанов
spans-col-count = Количество
spans-col-p50 = p50
spans-col-p95 = p95
spans-col-avg = Сред.
spans-agg-truncated = Показаны первые { $count } операций спанов.

# --- Таблица всех спанов ---
spans-col-span-id = ID спана
spans-col-op = Операция
spans-col-description = Описание
spans-col-timestamp = Метка времени

# --- Постраничная навигация (список спанов) ---
spans-pagination-label = Постраничная навигация
spans-traces-pagination-label = Постраничная навигация по трассам
spans-traces-count = { $count ->
    [one] { $count } трасса
    [few] { $count } трассы
    [many] { $count } трасс
   *[other] { $count } трассы
}
spans-pagination-prev = « Назад
spans-pagination-next = Вперёд »
spans-count = { $count ->
    [one] { $count } спан
    [few] { $count } спана
    [many] { $count } спанов
   *[other] { $count } спана
}

# --- Список трасс по всей организации ---
traces-col-root = Корневая транзакция
traces-col-projects = Проекты
traces-col-transactions = Транзакции
traces-col-errors = Ошибки
traces-list-empty = Трассы не найдены.
traces-filter-projects-label = ID проектов
traces-filter-projects-placeholder = ID проектов
traces-filter-period-label = Период
traces-filter-multi-label = Охватывает несколько проектов
traces-filter-submit = Фильтр
traces-pagination-label = Навигация по трассам
traces-count = { $count ->
    [one] { $count } трасса
    [few] { $count } трассы
    [many] { $count } трасс
   *[other] { $count } трассы
}
traces-more-projects = { $count ->
    [one] ещё +{ $count }
    [few] ещё +{ $count }
    [many] ещё +{ $count }
   *[other] ещё +{ $count }
}

# --- Деталь трейса (водопад) ---
# title-prefix/suffix оборачивают динамический ID трейса; total/showing-first/of
# разбиты по границам { $var } строки с мета-данными.
trace-detail-title-prefix = Трейс
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = всего
trace-detail-showing-first = показаны первые
trace-detail-of = из
trace-detail-empty = Спаны для этого трейса не найдены.
trace-detail-col-span = Спан
trace-detail-col-duration = Длительность
trace-detail-root-fallback = (корень трейса)
trace-detail-error-title = ошибка
trace-detail-span-fallback = спан
trace-detail-compressed-note = простои сжаты
trace-detail-gap-title = Свёрнутый простой (нет активных спанов)
trace-detail-lbl-span-id = ID спана
trace-detail-lbl-parent = Родительский спан
trace-detail-lbl-status = Статус
trace-detail-lbl-start = Смещение начала
trace-detail-correlated-errors = Связанные ошибки
trace-detail-col-level = Уровень
trace-detail-col-title = Заголовок
trace-detail-col-timestamp = Метка времени
trace-detail-span-count = { $count ->
    [one] { $count } спан
    [few] { $count } спана
    [many] { $count } спанов
   *[other] { $count } спана
}
trace-detail-legend-heading = Проекты
trace-detail-filter-active = отфильтровано
trace-detail-filter-clear = Показать все
trace-detail-col-project = Проект
trace-detail-parent-not-in-view = Родительский спан вне представления
trace-detail-txn-badge = Транзакция
trace-detail-view-events = Все события этого трейса
