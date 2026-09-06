# Интерфейс организаций: список организаций (templates/orgs.html), страница
# участников/приглашений (templates/org_members.html) и отдельная страница
# принятия приглашения (templates/invite_accept.html, ключи invite-*). Повторно
# использует nav-organizations и common-action-save. Разделительные пробелы
# находятся в шаблоне. Предложение об удалении и подпись «Введите <slug> для
# подтверждения» разделены по местам {{ var }}. Значения enum (member/owner,
# status) остаются в шаблоне без перевода.
orgs-page-title = Организации - Stackpit
orgs-subtitle = Организации, в которых вы состоите. Переключайтесь между ними или создайте новую.
orgs-empty = Вы пока не состоите ни в одной организации.
orgs-col-organization = Организация
orgs-col-kind = Тип
orgs-members-btn = Участники
orgs-active = Активна
orgs-switch = Переключить
orgs-create-heading = Создать организацию
orgs-create-desc = Вы становитесь владельцем. Если оставить поле пустым, slug формируется из названия.
orgs-name = Название
orgs-slug = Slug
orgs-optional = (необязательно)
orgs-create-submit = Создать организацию

# --- Страница участников ---
orgs-members-title-suffix = Участники - Stackpit
orgs-members-word = Участники
orgs-organization-word = организация
orgs-slug-heading = Slug
orgs-slug-desc = Идентифицирует эту организацию в URL. Должен быть уникальным.
orgs-email = Email
orgs-role = Роль
orgs-role-member = участник
orgs-role-owner = владелец
orgs-member-fallback = пользователь #{ $id }
orgs-joined = Присоединился
orgs-promote = Повысить
orgs-demote = Понизить
orgs-remove = Удалить
orgs-invites-heading = Приглашения
orgs-created = Создано
orgs-expires = Истекает
orgs-status = Статус
orgs-revoke = Отозвать
orgs-create-invite-heading = Создать приглашение
orgs-create-invite-desc = Создаёт одноразовую ссылку-приглашение.
orgs-expiry-label = Срок действия (секунды)
orgs-expiry-hint = (необязательно, по умолчанию 7 дней)
orgs-create-invite-submit = Создать приглашение
orgs-forseti-note = Членство в этой организации управляется извне.
orgs-personal-note = Это личная организация. Членство настроить нельзя.
orgs-danger-heading = Опасная зона
orgs-delete-danger-pre = При удалении будут удалены
orgs-delete-danger-projects = проект(ов),
orgs-delete-danger-members = участник(ов),
orgs-delete-danger-rest = а также все события, ошибки, ключи, оповещения и интеграции. Это действие необратимо.
orgs-confirm-type-pre = Введите
orgs-confirm-type-post = для подтверждения
orgs-delete-confirm = Удалить эту организацию и ВСЕ её данные. Это действие необратимо.
orgs-delete-submit = Удалить организацию

# --- Принятие приглашения (отдельная страница) ---
invite-page-title = Приглашение в организацию - Stackpit
invite-heading = Приглашение в организацию
invite-back-projects = Назад к проектам
invite-intro-pre = Вас пригласили присоединиться к
invite-intro-as = как
invite-intro-post = .
invite-accept-btn = Принять приглашение
invite-decline = Отклонить
invite-error-accepted = Это приглашение уже было принято.
invite-error-expired = Срок действия этого приглашения истёк.
invite-error-email-mismatch = Это приглашение предназначено для другого адреса электронной почты. Запросите приглашение без привязки к адресу или войдите с соответствующей учётной записью.

# Сообщения проверки/ошибок для страницы html_error, переводятся в местах
# вызова, несущих локаль запроса. Внутренние ошибки 5xx остаются на английском.
orgs-err-name-required = Название организации обязательно.
orgs-err-slug-taken = Этот slug уже занят.
orgs-err-invite-not-found = Приглашение не найдено или недействительно.
orgs-err-org-not-found = Организация не найдена.
orgs-err-last-owner-remove = Последнего владельца нельзя удалить.
orgs-err-last-owner-demote = Последнего владельца нельзя понизить.
orgs-err-confirm-slug = Введите slug организации для подтверждения удаления.
orgs-err-not-deletable = Эту организацию нельзя удалить.
orgs-err-license-cap-reached = Достигнут лимит организаций вашей лицензии. Удалите организацию или обновите лицензию, чтобы создать ещё одну.
orgs-err-limit-reached = { $count ->
    [one] Вы достигли лимита в { $count } организацию.
    [few] Вы достигли лимита в { $count } организации.
    [many] Вы достигли лимита в { $count } организаций.
   *[other] Вы достигли лимита в { $count } организации.
}
