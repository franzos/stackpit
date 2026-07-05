# Superficie impostazioni integrazioni: l'elenco (templates/integrations.html) e
# i tre form di aggiunta (webhook, slack, email). Usa nav-settings/nav-integrations
# per la chrome. Gli spazi separatori stanno nel template. integrations-empty
# contiene markup <strong> inline e il glifo della freccia, renderizzato con |safe.
integrations-page-title = Integrazioni — Stackpit
integrations-subtitle = Output verso Webhook, Slack ed email. Il routing per progetto si imposta nelle impostazioni di ciascun progetto.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + Email
integrations-empty = Ancora nessuna integrazione. Aggiungine una sopra per iniziare a ricevere notifiche. Dopo averla aggiunta, abilitala per ogni progetto in <strong>Impostazioni progetto → Integrazioni</strong>.
integrations-col-name = Nome
integrations-col-type = Tipo
integrations-col-endpoint = Endpoint
integrations-col-created = Creata
integrations-delete-confirm = Eliminare questa integrazione? Verrà rimossa da tutti i progetti.
integrations-test = Prova
integrations-delete = Elimina
flash-test-failed = Prova non riuscita: { $error }

# Etichette/pulsanti dei form condivisi tra i tre form di aggiunta.
integrations-cancel = Annulla
integrations-optional = (facoltativo)
integrations-required = (obbligatorio)
integrations-create = Crea integrazione

# --- Aggiungi webhook ---
integrations-webhook-title = Aggiungi webhook — Stackpit
integrations-webhook-breadcrumb = Aggiungi webhook
integrations-webhook-heading = Aggiungi integrazione webhook
integrations-webhook-name-placeholder = es. Avvisi di produzione
integrations-webhook-url-label = URL del webhook
integrations-webhook-secret-label = Secret HMAC
integrations-webhook-secret-placeholder = Secret di firma facoltativo

# --- Aggiungi Slack ---
integrations-slack-title = Aggiungi Slack — Stackpit
integrations-slack-breadcrumb = Aggiungi Slack
integrations-slack-heading = Aggiungi integrazione Slack
integrations-slack-name-placeholder = es. canale #alerts
integrations-slack-url-label = URL del webhook Slack

# --- Aggiungi email ---
integrations-email-title = Aggiungi email — Stackpit
integrations-email-breadcrumb = Aggiungi email
integrations-email-heading = Aggiungi integrazione email
integrations-email-name-placeholder = es. Avvisi email del team
integrations-email-lock-pre = Il provider e il mittente provengono dalla
integrations-email-lock-post = configurazione del server; questa integrazione sceglie solo il destinatario.
integrations-email-provider-label = Provider
integrations-email-token-label = Token API
integrations-email-token-placeholder-default = Lascia vuoto per usare il valore predefinito
integrations-email-token-placeholder = Token API del provider
integrations-email-from-label = Indirizzo mittente
integrations-email-fromname-label = Nome mittente
integrations-email-smtp-hint = SMTP usa la connessione [email.smtp] del server; non serve un token per integrazione.
