# Anmeldeseite (templates/login.html) sowie die OAuth-/Logout-Bannertexte
# aus src/html/login.rs. login-token-help enthält Inline-<code>-Markup und
# wird mit |safe gerendert.
login-page-title = Anmelden — Stackpit
login-welcome = Willkommen zurück
login-subtitle = Melde dich an, um dein Fehler-Tracking zu verwalten
login-sso = Mit SSO anmelden
login-or = oder
login-token-label = Admin-Token
login-token-placeholder = Master-Token eingeben…
login-submit = Anmelden
login-token-help = Das Admin-Token stammt aus <code class="text-mono">admin_token</code> in <code class="text-mono">stackpit.toml</code>. Bearbeite die Datei und starte <code class="text-mono">stackpit serve</code> neu, um Änderungen zu übernehmen.
login-docs = Dokumentation
login-selfhosting = Anleitung zum Selbst-Hosten

# Fehler-Banner (aus OAuth-?error=-Codes abgeleitet) und Logout-Hinweis-Banner.
login-error-state-mismatch = Deine Anmeldesitzung wurde manipuliert oder ist abgelaufen. Bitte versuche es erneut.
login-error-session-expired = Deine Sitzung ist abgelaufen. Bitte melde dich erneut an.
login-error-missing-response = Dein Identitätsanbieter hat eine unvollständige Antwort zurückgegeben. Bitte versuche es erneut.
login-error-token-exchange = Wir konnten die Anmeldung bei deinem Identitätsanbieter nicht abschließen. Bitte versuche es gleich noch einmal.
login-error-provisioning = Dein Konto konnte nicht erstellt werden. Wende dich an deinen Administrator.
login-error-email-conflict = Ein Konto mit dieser E-Mail-Adresse existiert bereits. Wende dich an deinen Administrator.
login-error-session-unavailable = Die Anmeldung ist vorübergehend nicht verfügbar. Bitte versuche es gleich noch einmal.
login-error-encryption = Die Anmeldung ist auf dieser Instanz falsch konfiguriert. Wende dich an deinen Administrator.
login-error-generic = Anmeldung fehlgeschlagen. Bitte versuche es erneut.
login-error-invalid-token = Ungültiges Token
login-logout-local = Von Stackpit abgemeldet. Deine Sitzung beim Identitätsanbieter wurde nicht beendet -- melde dich dort bei Bedarf separat ab.
