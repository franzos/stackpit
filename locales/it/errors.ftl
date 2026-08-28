# Corrispondenze italiane di locales/en/errors.ftl. Il marchio "Stackpit" resta
# letterale nei template, come in base.html/login.html.
error-page-title = Errore - Stackpit
error-heading = Errore
error-not-found = La pagina richiesta non esiste.
error-back-projects = Torna ai progetti

# Pagina di conferma per gli inviti creati (solo inglese/locale predefinito).
invite-created-page-title = Invito creato - Stackpit
invite-created-heading = Invito creato
invite-created-share = Condividi questo link. È valido per { $ttl } e utilizzabile una sola volta.
invite-created-back-members = Torna ai membri

# --- Messaggi flash, di successo e di validazione (dipendenti dal locale) ---

# Diagnostica non-trovato. Il prefisso "Errore:" viene anteposto in Rust; il
# valore contiene solo l'entità e l'id.
flash-not-found-project = progetto non trovato: { $id }
flash-not-found-key = chiave API non trovata: { $id }
flash-not-found-integration = integrazione non trovata: { $id }
flash-not-found-alert-rule = regola di avviso non trovata: { $id }
flash-not-found-digest-schedule = pianificazione di riepilogo non trovata: { $id }
flash-not-found-repo = repository non trovato: { $id }
flash-not-found-project-integration = integrazione del progetto non trovata: { $id }
flash-not-found-filter = { $label } non trovato

# Validazione delle regole di filtro
flash-unrecognized-field = Campo non riconosciuto: { $value }
flash-unrecognized-operator = Operatore non riconosciuto: { $value }
flash-unrecognized-action = Azione non riconosciuta: { $value }

# Impostazioni del progetto
flash-project-name-updated = Nome del progetto aggiornato
flash-project-name-too-long = Il nome del progetto supera la lunghezza massima di { $max } caratteri
flash-repo-url-required = L'URL del repository è obbligatorio
flash-repo-url-too-long = L'URL del repository supera la lunghezza massima di 2048 caratteri
flash-repo-added = Repository aggiunto
flash-repo-removed = Repository rimosso
flash-project-archived = Progetto archiviato
flash-project-unarchived = Progetto ripristinato dall'archivio
flash-key-created = Chiave creata
flash-key-deleted = Chiave eliminata

# Avvisi e riepiloghi
flash-project-not-found-or-denied = Errore: progetto non trovato o accesso negato
flash-alert-rule-created = Regola di avviso creata
flash-alert-rule-deleted = Regola di avviso eliminata
flash-digest-schedule-created = Pianificazione di riepilogo creata
flash-digest-schedule-deleted = Pianificazione di riepilogo eliminata

# Integrazioni del progetto
flash-integration-not-found = Integrazione non trovata
flash-integration-activated = Integrazione attivata
flash-integration-updated = Integrazione aggiornata
flash-integration-deactivated = Integrazione disattivata

# Integrazioni dell'organizzazione
flash-name-required = Il nome è obbligatorio
flash-invalid-integration-kind = Tipo di integrazione non valido
flash-invalid-email-provider = Provider email non valido
flash-api-token-required = Il token API è obbligatorio.
flash-from-address-required = L'indirizzo del mittente è obbligatorio.
flash-smtp-not-configured = SMTP non è configurato. Imposta [email] host nella configurazione del server.
flash-invalid-to-address = Il destinatario deve essere un indirizzo email valido.
flash-test-digest-sent = Digest di prova in coda per { $count } progetto/i verso le integrazioni con digest abilitati.
flash-test-digest-sample = Nessuna attività recente, quindi è stato messo in coda un digest di esempio etichettato.
flash-test-digest-no-target = Nessuna integrazione ha i digest abilitati per il progetto di questa pianificazione.
flash-url-required = L'URL è obbligatorio
flash-secret-not-configured = Impossibile salvare il secret: la crittografia non è configurata. Imposta STACKPIT_MASTER_KEY per abilitare l'archiviazione dei secret.
flash-integration-license-required = Le integrazioni Slack, webhook e issue tracker richiedono una licenza commerciale attiva. Le notifiche via e-mail restano disponibili anche senza licenza.
flash-integration-created = Integrazione creata
flash-integration-name-exists = Esiste già un'integrazione con questo nome.
flash-integration-deleted = Integrazione eliminata
flash-integration-no-url = Per l'integrazione non è configurato alcun URL
flash-test-notification-sent = Notifica di prova inviata

# Filtri in entrata
flash-inbound-filters-updated = Filtri in entrata aggiornati
flash-pattern-required = Il pattern è obbligatorio
flash-message-filter-added = Filtro messaggi aggiunto
flash-message-filter-removed = Filtro messaggi rimosso
flash-rate-limit-updated = Limite di frequenza aggiornato
flash-environment-required = L'ambiente è obbligatorio
flash-environment-excluded = Ambiente escluso
flash-environment-filter-removed = Filtro ambiente rimosso
flash-release-filter-added = Filtro release aggiunto
flash-release-filter-removed = Filtro release rimosso
flash-ua-filter-added = Filtro User-Agent aggiunto
flash-ua-filter-removed = Filtro User-Agent rimosso
flash-rule-added = Regola aggiunta
flash-rule-removed = Regola rimossa
flash-cidr-required = Il CIDR è obbligatorio
flash-invalid-cidr = Formato CIDR non valido
flash-ip-block-added = Blocco IP aggiunto
flash-ip-block-removed = Blocco IP rimosso

# Nuovo progetto
flash-project-name-required = Il nome del progetto è obbligatorio
flash-email-not-configured = L'email non è configurata. Aggiungi una sezione [email] con un provider alla configurazione del server.
flash-integration-saved = Integrazione aggiornata
flash-integration-global-not-for-trackers = I tracker di issue non usano l'instradamento a livello di organizzazione; il repository di destinazione viene dalle impostazioni repository di ogni progetto.
flash-project-excluded = Progetto escluso da questa integrazione
flash-project-included = Progetto non più escluso
flash-global-email-needs-recipient = Un'integrazione email a livello di organizzazione richiede un destinatario predefinito; i progetti che non l'hanno mai attivata non hanno un indirizzo proprio.
flash-queue-item-not-found = Notifica in coda non trovata
flash-queue-replayed = Notifica consegnata e rimossa dalla coda
flash-queue-replay-failed = Reinvio non riuscito: { $error }
flash-queue-cancelled = Notifica in coda scartata
flash-queue-replay-failed-generic = Nuovo invio fallito. Il motivo è sull'elemento in coda, sotto Errore.
flash-license-activated = Licenza attivata
flash-license-deactivated = Licenza rimossa
flash-license-persist-failed = La licenza è stata verificata ma non si è potuta salvare. Controlla il log del server.
flash-license-clear-failed = Non è stato possibile rimuovere la licenza. Controlla il log del server.
flash-license-empty = Incolla la tua chiave di licenza per attivarla.
flash-license-bad-signature = Questa licenza non è valida per questa installazione. Verifica di aver incollato la chiave giusta.
flash-license-wrong-product = Questa licenza non è per Stackpit.
flash-license-unreadable = Non è stato possibile leggere quella licenza. Controllala e riprova.
