# Метрики: список метрик по проекту и страница деталей временного ряда
# метрики. Использует nav-metrics. Строки со счётчиком используют плюрали tv_count.

# --- Суффикс заголовка страницы ---
metrics-title-suffix = — Stackpit

# --- Список метрик ---
metrics-list-empty = Метрики не найдены. События метрик появятся здесь, как только будут получены.
metrics-col-mri = MRI
metrics-col-type = Тип
metrics-col-data-points = Точки данных
metrics-col-first-seen = Впервые замечено
metrics-col-last-seen = Последний раз замечено

# --- Постраничная навигация ---
metrics-pagination-label = Постраничная навигация
metrics-pagination-prev = « Назад
metrics-pagination-next = Далее »
metrics-count = { $count ->
    [one] { $count } метрика
    [few] { $count } метрики
    [many] { $count } метрик
   *[other] { $count } метрики
}

# --- Детали метрики (почасовые интервалы) ---
metrics-detail-empty = Нет точек данных в выбранном периоде времени.
metrics-detail-col-time = Время (почасовой интервал)
metrics-detail-col-count = Количество
metrics-detail-col-sum = Сумма
metrics-detail-col-min = Мин
metrics-detail-col-max = Макс
metrics-detail-col-avg = Сред
