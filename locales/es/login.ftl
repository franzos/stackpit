# Página de inicio de sesión (templates/login.html) y los textos del banner de
# OAuth/cierre de sesión de src/html/login.rs. login-token-help contiene markup
# <code> en línea y se renderiza con |safe.
login-page-title = Iniciar sesión — Stackpit
login-welcome = Bienvenido de nuevo
login-subtitle = Inicia sesión para gestionar tu seguimiento de errores
login-sso = Iniciar sesión con SSO
login-or = o
login-token-label = Token de administrador
login-token-placeholder = Introduce tu token maestro…
login-submit = Iniciar sesión
login-token-help = El token de administrador proviene de <code class="text-mono">admin_token</code> en <code class="text-mono">stackpit.toml</code>. Edita el archivo y reinicia <code class="text-mono">stackpit serve</code> para aplicar los cambios.
login-docs = Documentación
login-selfhosting = Guía de autoalojamiento

# Banner de error (derivado de los códigos ?error= de OAuth) y banner informativo de cierre de sesión.
login-error-state-mismatch = Tu sesión de inicio fue manipulada o expiró. Inténtalo de nuevo.
login-error-session-expired = Tu sesión expiró. Vuelve a iniciar sesión.
login-error-missing-response = Tu proveedor de identidad devolvió una respuesta incompleta. Inténtalo de nuevo.
login-error-token-exchange = No pudimos completar el inicio de sesión con tu proveedor de identidad. Inténtalo de nuevo en un momento.
login-error-provisioning = No se pudo crear tu cuenta. Contacta con tu administrador.
login-error-email-conflict = Ya existe una cuenta con este correo electrónico. Contacta con tu administrador.
login-error-session-unavailable = El inicio de sesión no está disponible temporalmente. Inténtalo de nuevo en un momento.
login-error-encryption = El inicio de sesión está mal configurado en esta instancia. Contacta con tu administrador.
login-error-generic = Error al iniciar sesión. Inténtalo de nuevo.
login-error-invalid-token = Token no válido
login-logout-local = Sesión cerrada en Stackpit. La sesión en tu proveedor de identidad no se cerró -- cierra sesión allí por separado si es necesario.
