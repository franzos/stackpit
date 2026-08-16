# Страница оповещений и сводок (templates/alerts.html). Использует nav-settings
# и nav-alerts-digests для элементов оформления. Разделительные пробелы находятся
# в шаблоне, поэтому значения не содержат ведущих/замыкающих пробелов.
# alerts-page-title содержит сырую сущность &amp; и рендерится с |safe.
alerts-page-title = Оповещения &amp; сводки — Stackpit
alerts-notify-help-pre = Уведомления срабатывают через интеграции на
alerts-notify-help-post = странице.

# --- Типы уведомлений ---
alerts-notify-types-heading = Типы уведомлений
alerts-notify-types-desc = Уведомления о новых и повторно возникших проблемах срабатывают для каждой впервые замеченной или регрессировавшей проблемы. Пороговые правила срабатывают по объёму событий в окне; сводки — это периодические обзоры. В этом списке только интеграции, подключённые самим проектом, — интеграция уровня организации доставляет во все проекты и настраивается на странице интеграций.
alerts-notify-types-empty = Ни один проект не подключил собственную интеграцию. Интеграции уровня организации здесь не показаны и при этом могут доставлять уведомления; откройте страницу интеграций.
alerts-col-integration = Интеграция
alerts-col-new-issues = Новые проблемы
alerts-col-regressions = Регрессии
alerts-col-digests = Сводки
alerts-notify-save = Сохранить

# --- Пороговые правила ---
alerts-threshold-heading = Пороговые правила
alerts-threshold-desc = Срабатывает, когда проблема получает более N событий за интервал времени.
alerts-rules-empty = Пока нет правил оповещений.
alerts-col-scope = Область
alerts-col-issue = Проблема
alerts-col-threshold = Порог
alerts-col-window = Окно
alerts-col-cooldown = Задержка
alerts-scope-global = Глобально
alerts-fingerprint-any = Любой
alerts-rule-delete-confirm = Удалить это правило оповещения?
alerts-delete-label = Удалить
alerts-add-rule = + Добавить правило оповещения
alerts-all-projects = Все проекты
alerts-project-fallback = Проект { $id }
alerts-fingerprint-label = Отпечаток проблемы
alerts-fingerprint-hint = (пусто = любой)
alerts-fingerprint-placeholder = любая проблема
alerts-fingerprint-help = Отпечаток идентифицирует одну проблему (сгруппированные события). Виден в URL на странице любой проблемы. Оставьте пустым, чтобы охватить каждую проблему в области.
alerts-unit-s = (с)
alerts-create-rule = Создать правило

# --- Расписания сводок ---
alerts-digest-heading = Расписания сводок
alerts-digest-desc = Периодические сводки активности — ежедневные или еженедельные отчёты вместо шума по каждому событию.
alerts-digests-empty = Пока нет расписаний сводок.
alerts-col-interval = Интервал
alerts-col-last-sent = Последняя отправка
alerts-col-enabled = Включено
alerts-never = Никогда
alerts-yes = Да
alerts-no = Нет
alerts-digest-delete-confirm = Удалить это расписание сводок?
alerts-add-digest = + Добавить расписание сводок
alerts-interval-daily = Ежедневно (24ч)
alerts-interval-weekly = Еженедельно (7д)
alerts-interval-hourly = Ежечасно
alerts-create-schedule = Создать расписание
