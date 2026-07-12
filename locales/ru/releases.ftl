# Раздел релизов: межпроектный список релизов и постраничная страница
# состояния релизов по проекту. Повторно использует nav-releases и nav-health.
# Счётные строки используют плюрали tv_count ([one]/[few]/[many]/[other]).

# --- Суффикс заголовка страницы ---
releases-title-suffix = — Stackpit

# --- Список релизов ---
releases-list-search-placeholder = Поиск релизов…
releases-list-search-label = Поиск релизов
releases-list-project-placeholder = ID проекта
releases-list-project-label = Фильтр по проекту
releases-list-period-label = Период освоения
releases-list-period-24h = Последние 24 ч
releases-list-period-7d = Последние 7 дней
releases-list-period-30d = Последние 30 дней
releases-filter-submit = Фильтровать
releases-list-empty = Релизов пока нет. Задайте <code class="text-mono">release</code> в своём SDK, и они появятся здесь, как только начнут поступать события.
releases-col-version = Версия
releases-col-project = Проект
releases-col-issues = Ошибки
releases-col-events = События
releases-col-adoption = Освоение
releases-col-first-seen = Первое появление
releases-col-last-seen = Последнее появление

# --- Постраничная навигация ---
releases-pagination-label = Постраничная навигация
releases-pagination-prev = « Назад
releases-pagination-next = Вперёд »
releases-count = { $count ->
    [one] { $count } релиз
    [few] { $count } релиза
    [many] { $count } релизов
   *[other] { $count } релиза
}

# --- Состояние релиза ---
release-health-title = Состояние релиза
release-health-heading = Состояние релиза
release-health-sessions-heading = Сессии во времени
release-health-empty = Данные о сессиях недоступны. События сессий с полем <code class="text-mono">status</code> появятся здесь.
release-health-col-release = Релиз
release-health-col-sessions = Сессии
release-health-col-ok = OK
release-health-col-crashed = Сбои
release-health-col-errored = С ошибками
release-health-col-crash-free-sessions = Сессии без сбоев
release-health-col-error-free-sessions = Сессии без ошибок
release-health-col-crash-free-users = Пользователи без сбоев
release-health-subtitle = Итоги сессий — это сигналы состояния от SDK, а не события ошибок. Нажмите на релиз, чтобы увидеть его ошибки.
release-health-crashed-title = Показать ошибки этого релиза
release-health-errored-title = Показать ошибки этого релиза
release-health-errored-hint = «С ошибками» — это сигналы состояния сессий от SDK (сессия, зафиксировавшая обработанную ошибку, но не давшая сбой), а не отдельные события ошибок, и их нельзя перечислить по сессиям. Связанные ошибки — это группы ошибок, замеченные в этом релизе.

# --- Детали релиза (по версии) ---
release-detail-sessions-heading = Состояние сессий
release-detail-sessions-note = Итоги сессий от SDK (ok / с ошибками / сбои). Это сигналы состояния, а не отдельные события ошибок.
release-detail-no-health = Нет данных о сессиях для этого релиза.
release-detail-issues-heading = Ошибки в этом релизе
release-detail-issues-note = Отдельные группы ошибок, впервые или последний раз замеченные с этим релизом.
release-detail-no-issues = Для этого релиза ошибок не зафиксировано.
release-health-na = н/д
