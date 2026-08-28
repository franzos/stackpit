# Стартовые ключи для сквозной проверки пайплайна. Полная выборка в P1a.
common-action-save = Сохранить
common-error-prefix = Ошибка:
nav-logout = Выйти
common-id-prefix = id:
common-time-just-now = только что
common-time-min-ago = { $n } мин назад
common-time-hour-ago = { $n } ч назад
common-time-week-ago = { $n } нед назад
common-time-month-ago = { $n } мес назад
common-time-year-ago = { $n } г назад
common-time-day-ago = { $n ->
    [one] { $n } день назад
    [few] { $n } дня назад
    [many] { $n } дней назад
   *[other] { $n } дня назад
}
common-period-all = За всё время
common-period-1h = Последний час
common-period-24h = Последние 24 часа
common-period-7d = Последние 7 дней
common-period-14d = Последние 14 дней
common-period-30d = Последние 30 дней
common-period-90d = Последние 90 дней
common-period-365d = Последние 365 дней

common-select-all-matching = { $count ->
    [one] Выбрать { $count } строку, соответствующую фильтру
    [few] Выбрать все { $count } строки, соответствующие фильтру
    [many] Выбрать все { $count } строк, соответствующих фильтру
   *[other] Выбрать все { $count } строки, соответствующие фильтру
}

test-count = { $count ->
    [one] { $count } элемент
    [few] { $count } элемента
    [many] { $count } элементов
   *[other] { $count } элемента
}
common-time-in-secs = через { $n } с
common-time-in-min = через { $n } мин
common-time-in-hour = через { $n } ч
common-time-in-day = { $n ->
    [one] через { $n } день
    [few] через { $n } дня
    [many] через { $n } дней
   *[other] через { $n } дня
}
common-pagination-prev = Назад
common-pagination-next = Вперёд
