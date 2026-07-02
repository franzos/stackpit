# Отдельная страница входа (templates/login.html) плюс тексты баннеров
# OAuth/выхода из src/html/login.rs. login-token-help содержит встроенную
# разметку <code> и рендерится с |safe.
login-page-title = Вход — Stackpit
login-welcome = С возвращением
login-subtitle = Войдите, чтобы управлять отслеживанием ошибок
login-sso = Войти через SSO
login-or = или
login-token-label = Токен администратора
login-token-placeholder = Введите мастер-токен…
login-submit = Войти
login-token-help = Токен администратора берётся из <code class="text-mono">admin_token</code> в <code class="text-mono">stackpit.toml</code>. Отредактируйте файл и перезапустите <code class="text-mono">stackpit serve</code>, чтобы применить изменения.
login-docs = Документация
login-selfhosting = Руководство по самостоятельному хостингу

# Баннер ошибки (сопоставленный с кодами ?error= редиректа OAuth) и информационный баннер выхода.
login-error-state-mismatch = Ваша сессия входа была подделана или истекла. Пожалуйста, попробуйте снова.
login-error-session-expired = Ваша сессия истекла. Пожалуйста, войдите снова.
login-error-missing-response = Ваш провайдер идентификации вернул неполный ответ. Пожалуйста, попробуйте снова.
login-error-token-exchange = Не удалось завершить вход через ваш провайдер идентификации. Пожалуйста, попробуйте снова через мгновение.
login-error-provisioning = Не удалось создать вашу учётную запись. Обратитесь к администратору.
login-error-email-conflict = Учётная запись с этим адресом e-mail уже существует. Обратитесь к администратору.
login-error-session-unavailable = Вход временно недоступен. Пожалуйста, попробуйте снова через мгновение.
login-error-encryption = Вход неправильно настроен в этой инсталляции. Обратитесь к администратору.
login-error-generic = Не удалось войти. Пожалуйста, попробуйте снова.
login-error-invalid-token = Недействительный токен
login-logout-local = Выполнен выход из Stackpit. Ваша сессия у провайдера идентификации не была завершена -- при необходимости выйдите там отдельно.
