# Pagina di accesso autonoma (templates/login.html) e i testi dei banner
# OAuth/logout prodotti in src/html/login.rs. login-token-help contiene markup
# <code> inline ed è renderizzato con |safe.
login-page-title = Accedi — Stackpit
login-welcome = Bentornato
login-subtitle = Accedi per gestire il monitoraggio degli errori
login-sso = Accedi con SSO
login-or = oppure
login-token-label = Token amministratore
login-token-placeholder = Inserisci il tuo master token…
login-submit = Accedi
login-token-help = Il token amministratore proviene da <code class="text-mono">admin_token</code> in <code class="text-mono">stackpit.toml</code>. Modifica il file e riavvia <code class="text-mono">stackpit serve</code> per applicare le modifiche.
login-docs = Documentazione
login-selfhosting = Guida al self-hosting

# Banner di errore (derivato dai codici OAuth ?error=) e banner informativo di logout.
login-error-state-mismatch = La tua sessione di accesso è stata manomessa o è scaduta. Riprova.
login-error-session-expired = La tua sessione è scaduta. Accedi di nuovo.
login-error-missing-response = Il tuo provider di identità ha restituito una risposta incompleta. Riprova.
login-error-token-exchange = Non è stato possibile completare l'accesso con il tuo provider di identità. Riprova tra un momento.
login-error-provisioning = Non è stato possibile creare il tuo account. Contatta l'amministratore.
login-error-email-conflict = Esiste già un account con questa email. Contatta l'amministratore.
login-error-session-unavailable = L'accesso è temporaneamente non disponibile. Riprova tra un momento.
login-error-encryption = L'accesso non è configurato correttamente su questo deployment. Contatta l'amministratore.
login-error-generic = Accesso non riuscito. Riprova.
login-error-invalid-token = Token non valido
login-logout-local = Disconnesso da Stackpit. La sessione presso il tuo provider di identità non è stata terminata -- disconnettiti lì separatamente se necessario.
