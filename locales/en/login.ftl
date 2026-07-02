# Standalone login page (templates/login.html) plus the OAuth/logout banner
# strings produced in src/html/login.rs. login-token-help carries inline
# <code> markup and is rendered with |safe.
login-page-title = Sign in — Stackpit
login-welcome = Welcome back
login-subtitle = Sign in to manage your error tracking
login-sso = Sign in with SSO
login-or = or
login-token-label = Admin Token
login-token-placeholder = Enter your master token…
login-submit = Sign in
login-token-help = The admin token comes from <code class="text-mono">admin_token</code> in <code class="text-mono">stackpit.toml</code>. Edit the file and restart <code class="text-mono">stackpit serve</code> to pick up changes.
login-docs = Documentation
login-selfhosting = Self-hosting guide

# Error banner (mapped from OAuth redirect ?error= codes) and logout info banner.
login-error-state-mismatch = Your sign-in session was tampered with or expired. Please try again.
login-error-session-expired = Your session expired. Please sign in again.
login-error-missing-response = Your identity provider returned an incomplete response. Please try again.
login-error-token-exchange = We couldn't complete sign-in with your identity provider. Please try again in a moment.
login-error-provisioning = Your account couldn't be created. Contact your administrator.
login-error-email-conflict = An account with this email already exists. Contact your administrator.
login-error-session-unavailable = Sign-in is temporarily unavailable. Please try again in a moment.
login-error-encryption = Sign-in is misconfigured on this deployment. Contact your administrator.
login-error-generic = Sign-in failed. Please try again.
login-error-invalid-token = Invalid token
login-logout-local = Signed out of Stackpit. Your identity provider session was not ended -- sign out there separately if needed.
