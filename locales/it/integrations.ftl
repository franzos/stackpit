# Superficie impostazioni integrazioni: l'elenco (templates/integrations.html) e
# i tre form di aggiunta (webhook, slack, email). Usa nav-settings/nav-integrations
# per la chrome. Gli spazi separatori stanno nel template. integrations-empty
# contiene markup <strong> inline e il glifo della freccia, renderizzato con |safe.
integrations-page-title = Integrazioni — Stackpit
integrations-subtitle = Output verso Webhook, Slack ed email. Il routing per progetto si imposta nelle impostazioni di ciascun progetto.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + Email
integrations-license-required-badge = Licenza richiesta
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
integrations-email-smtp-hint = SMTP usa la connessione [email] del server; non serve un token per integrazione.

# Tracker di issue
integrations-add-tracker = + Tracker di issue
integrations-tracker-title = Aggiungi tracker di issue — Stackpit
integrations-tracker-breadcrumb = Aggiungi tracker di issue
integrations-tracker-heading = Aggiungi integrazione tracker di issue
integrations-tracker-kind-label = Tracker
integrations-tracker-name-placeholder = es. GitHub Issues
integrations-tracker-url-label = URL di base
integrations-tracker-token-label = Token API
integrations-tracker-token-placeholder = Token di accesso personale
integrations-tracker-target-help = Il repository di destinazione viene dalle impostazioni repository di ogni progetto, quindi non si configura qui. Aggiungi il repository nelle impostazioni del progetto.
integrations-global-label = Consegna a tutti i progetti
integrations-global-help = Gli avvisi vanno a tutti i progetti di questa organizzazione, tranne quelli che escludi nella pagina di questa integrazione. I filtri di livello e ambiente per progetto restano validi in aggiunta.
integrations-global-badge = intera organizzazione
integrations-global-save = Salva instradamento
integrations-global-on = Consegna a tutta l'organizzazione
integrations-global-off = Interrompi la consegna a tutta l'organizzazione

# Dettaglio integrazione: instradamento per progetto
integrations-detail-title = Integrazione — Stackpit
integrations-back = Torna alle integrazioni
integrations-projects-heading = Instradamento per progetto
integrations-projects-hint-global = Questa integrazione consegna a tutti i progetti qui sotto, a meno che non li escluda. L'esclusione è l'unica via d'uscita; non esiste una lista di inclusione.
integrations-projects-hint-per-project = Questa integrazione consegna solo dove un progetto l'ha attivata. Contrassegnala per l'intera organizzazione per consegnare ovunque.
integrations-projects-hint-tracker = I tracker di issue vengono abbinati ai repository di un progetto per forge e host. Escludere un progetto toglie questo tracker dalle sue opzioni di creazione.
integrations-projects-empty = Questa organizzazione non ha ancora progetti.
integrations-col-project = Progetto
integrations-col-state = Stato
integrations-project-archived = archiviato
integrations-state-default = In consegna
integrations-state-customised = Personalizzato
integrations-state-excluded = Escluso
integrations-state-no-repo = Nessun repository corrispondente
integrations-state-not-routed = Non attivato
integrations-exclude = Escludi
integrations-include = Includi
integrations-email-to-label = Destinatario predefinito
integrations-email-to-help = Usato dove un progetto non ha impostato un proprio indirizzo. Obbligatorio per un'integrazione a livello di organizzazione.
integrations-summary-delivering = { $count ->
    [one] { $count } consegna
   *[other] { $count } consegnano
}
integrations-summary-excluded = { $count ->
    [one] { $count } escluso
   *[other] { $count } esclusi
}
integrations-summary-inert = { $count ->
    [one] { $count } senza consegna
   *[other] { $count } senza consegna
}
integrations-search-placeholder = Filtra per nome progetto
integrations-search-label = Filtra i progetti
integrations-search-submit = Filtra
integrations-sort-label = Ordina i progetti
integrations-sort-state = Prima quelli che consegnano
integrations-sort-name = Per nome
integrations-pagination-label = Pagine della consegna per progetto
integrations-projects-count = { $count ->
    [one] { $count } progetto
   *[other] { $count } progetti
}
