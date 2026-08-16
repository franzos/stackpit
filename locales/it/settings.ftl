# Superficie impostazioni: la pagina dei valori predefiniti del browser
# (templates/browser_defaults.html, chiavi defaults-*) e la pagina autonoma di
# provisioning delle org (templates/provision.html, chiavi provision-*). Usa
# nav-settings. I valori di livello (fatal/error/...) restano nel template, come
# sulle superfici problemi/eventi dove i log level restano in inglese canonico.

# --- Valori predefiniti del browser ---
defaults-page-title = Valori predefiniti del browser — Stackpit
defaults-subtitle = Imposta i valori di filtro predefiniti per le pagine di elenco. Memorizzati come cookie del browser.
defaults-none = Nessun valore predefinito
defaults-status-label = Stato predefinito (problemi)
defaults-status-unresolved = Non risolto
defaults-status-resolved = Risolto
defaults-status-ignored = Ignorato
defaults-level-label = Livello predefinito
defaults-period-label = Intervallo di tempo predefinito
defaults-save = Salva valori predefiniti
defaults-clear-confirm = Cancellare tutti i valori predefiniti del browser?
defaults-clear = Cancella tutti i valori predefiniti
flash-defaults-saved = Valori predefiniti salvati
flash-defaults-cleared = Valori predefiniti cancellati

# --- Lingua preferita ---
settings-language-heading = Lingua preferita
settings-language-subtitle = Scegli la lingua dell'interfaccia di Stackpit. Gli account con cui hai effettuato l'accesso la mantengono su tutti i dispositivi.
settings-language-label = Lingua
settings-language-save = Salva lingua

settings-aria-sections = Sezioni delle impostazioni

# --- Pagina di provisioning (pagina autonoma) ---
provision-page-title = Configura le organizzazioni — Stackpit
provision-heading = Configura le organizzazioni
provision-subtitle-1 = Le seguenti organizzazioni sono disponibili dal tuo provider di identità.
provision-subtitle-2 = Seleziona quelle che vuoi creare in Stackpit.
provision-create = Crea selezionate
provision-skip = Salta

# Coda di consegna
queue-page-title = Coda di consegna — Stackpit
queue-subtitle = Notifiche che non è stato possibile consegnare. Vengono riprovate automaticamente per 24 ore, poi restano qui ad aspettarti.
queue-count-pending = { $count } in attesa
queue-count-failed = { $count } fallite
queue-empty = Niente in coda. Tutte le notifiche sono state consegnate.
queue-col-integration = Integrazione
queue-col-project = Progetto
queue-col-state = Stato
queue-col-attempts = Tentativi
queue-col-queued = In coda da
queue-col-error = Ultimo errore
queue-state-pending = Nuovo tentativo
queue-state-failed = Abbandonata
queue-replay = Reinvia
queue-cancel = Scarta
queue-cancel-confirm = Scartare questa notifica senza consegnarla?
