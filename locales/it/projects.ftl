# Superficie progetti: elenco, nuovo, impostazioni (generale/chiavi/sourcemap/
# filtri), integrazioni e la conferma di creazione. I valori renderizzati con
# |safe contengono markup HTML inline; i tag restano identici, solo il testo è
# tradotto.

# --- Elenco progetti ---
projects-list-title = Progetti — Stackpit
projects-list-heading = Progetti
projects-list-subtitle = Monitora lo stato dell'intera architettura.
projects-list-all-events = Tutti gli eventi
projects-list-all-releases = Tutte le release
projects-list-new = + Nuovo progetto
projects-list-search-placeholder = Cerca progetti per nome, piattaforma o proprietario…
projects-list-search-label = Cerca progetti
projects-list-filter = Filtra
projects-org-filter-label = Filtra per organizzazione
projects-org-filter-all = Tutte le organizzazioni
projects-list-empty = Nessun progetto trovato. Gli eventi appariranno qui una volta acquisiti.
projects-period-label = Intervallo di tempo
projects-col-project = Progetto
projects-col-platforms = Piattaforme
projects-col-issues = Problemi
projects-col-events = Eventi
projects-col-breakdown = Ripartizione
projects-col-release = Release
projects-col-first-seen = Prima occorrenza
projects-col-last-seen = Ultima occorrenza
projects-breakdown-errors = Errori:
projects-breakdown-transactions = Transazioni:
projects-breakdown-sessions = Sessioni:
projects-breakdown-other = Altro:
projects-legend-errors = Errori
projects-legend-transactions = Transazioni
projects-legend-sessions = Sessioni
projects-legend-other = Altro

# --- Condiviso tra i form dei progetti ---
projects-optional = (facoltativo)
projects-cancel = Annulla
projects-remove = Rimuovi
projects-delete = Elimina
projects-name-placeholder = Il mio progetto

# --- Nuovo progetto ---
projects-new-title = Nuovo progetto — Stackpit
projects-new-heading = Nuovo progetto
projects-new-name-label = Nome del progetto
projects-new-platform-label = Piattaforma
projects-new-platform-select = Seleziona una piattaforma…
projects-new-platform-other = Altra
projects-new-platform-native = Native (C/C++)
projects-new-submit = Crea progetto

# --- Tab delle impostazioni (condivisi dalle pagine di impostazioni) ---
projects-tab-general = Generale
projects-tab-sdk = Configurazione SDK
projects-tab-sourcemaps = Source map
projects-tab-filters = Filtri
projects-tab-integrations = Integrazioni

# --- Impostazioni: generale ---
projects-settings-heading = Impostazioni
projects-settings-archived = (archiviato)
projects-settings-name-heading = Nome del progetto
projects-settings-display-name = Nome visualizzato
projects-settings-save-name = Salva nome
projects-settings-info-heading = Informazioni sul progetto
projects-settings-status = Stato
projects-settings-source = Origine
projects-repos-heading = Repository di origine
projects-repos-help = Collega gli stack frame al codice sorgente sulla tua forge. Registra una release con uno SHA di commit tramite <code class="text-mono">sentry-cli</code> per attivare i collegamenti.
projects-repos-empty = Nessun repository configurato.
projects-repos-url-label = URL del repository
projects-repos-col-forge = Forge
projects-repos-template = Modello URL
projects-repos-auto = auto
projects-repos-remove-confirm = Rimuovere questo repository?
projects-repos-add = Aggiungi repository
projects-repos-add-help = Aggiunge collegamenti al sorgente cliccabili (es. "Visualizza su GitHub") accanto agli stack frame. Richiede una release con uno SHA di commit — il tipo di forge viene rilevato automaticamente. Supportati: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. Per altre forge, fornisci un modello URL.
projects-danger-heading = Zona pericolosa
projects-archive-desc = Archivia questo progetto. I progetti archiviati rifiutano i nuovi eventi.
projects-archive-confirm = Archiviare questo progetto? I nuovi eventi verranno rifiutati.
projects-archive-submit = Archivia progetto
projects-unarchive-desc = Ripristina questo progetto dall'archivio per riprendere ad accettare eventi.
projects-unarchive-submit = Ripristina progetto
projects-delete-desc = Elimina definitivamente questo progetto e tutti i suoi dati. L'operazione non può essere annullata.
projects-delete-confirm = Eliminare questo progetto e TUTTI i suoi dati? L'operazione non può essere annullata.
projects-delete-submit = Elimina progetto
projects-move-heading = Sposta in un'organizzazione
projects-move-desc = Sposta questo progetto in un'altra organizzazione di cui sei proprietario. I suoi dati e i DSN restano validi, ma le integrazioni di notifica vengono scollegate e devono essere aggiunte di nuovo nella nuova organizzazione.
projects-move-target-label = Organizzazione di destinazione
projects-move-confirm-pre = Digita
projects-move-confirm-post = per confermare.
projects-move-confirm-placeholder = Nome del progetto
projects-move-confirm-dialog = Spostare questo progetto nell'organizzazione selezionata?
projects-move-submit = Sposta progetto
projects-move-err-invalid-target = Organizzazione di destinazione non valida.
projects-move-err-name-mismatch = Il nome del progetto non corrisponde.
projects-move-err-denied = Non sei proprietario dell'organizzazione di destinazione.
projects-move-err-conflict = Impossibile spostare il progetto; potrebbe essere cambiato. Riprova.

# --- Impostazioni: configurazione SDK / chiavi ---
projects-keys-title = Configurazione SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = Nessuna chiave registrata. Crea una chiave qui sotto per ottenere un DSN.
projects-keys-list-heading = Chiavi del progetto
projects-keys-empty = Nessuna chiave registrata per questo progetto.
projects-keys-col-public = Chiave pubblica
projects-keys-col-label = Etichetta
projects-keys-col-status = Stato
projects-keys-col-created = Creata
projects-keys-delete-confirm = Eliminare questa chiave? Gli SDK che la utilizzano smetteranno di funzionare.
projects-keys-create-heading = Crea chiave
projects-keys-label-label = Etichetta
projects-keys-label-placeholder = es. production, staging
projects-keys-create-submit = Crea chiave

# --- Impostazioni: source map ---
projects-sourcemaps-title = Source map
projects-sourcemaps-apikey-heading = Chiave API
projects-sourcemaps-apikey-desc = Il caricamento delle source map richiede una chiave API. Specifica per questo progetto e utilizzabile solo per operazioni sulle source map.
projects-sourcemaps-key-generated = Chiave generata:
projects-sourcemaps-key-warning = Copia questa chiave ora — non verrà più mostrata.
projects-sourcemaps-col-key = Chiave
projects-sourcemaps-regen-confirm = Rigenerare la chiave? La chiave attuale smetterà di funzionare.
projects-sourcemaps-regen = Rigenera
projects-sourcemaps-empty = Nessuna chiave API per le source map di questo progetto.
projects-sourcemaps-generate = Genera chiave
projects-sourcemaps-setup-heading = Configurazione
projects-sourcemaps-setup-desc = Usa <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> per caricare le source map. Imposta queste variabili d'ambiente:
projects-sourcemaps-then-upload = Poi carica:

# --- Impostazioni: filtri ---
projects-filters-inbound-heading = Filtri in entrata
projects-filters-inbound-desc = Filtri integrati che scartano gli eventi corrispondenti a comuni pattern di rumore.
projects-filters-browser-ext = Estensioni del browser — scarta gli eventi provenienti dalle estensioni di Chrome/Firefox/Safari
projects-filters-localhost = Localhost — scarta gli eventi provenienti da localhost, 127.0.0.1, IP privati
projects-filters-inbound-submit = Salva filtri in entrata
projects-filters-message-heading = Filtri messaggi
projects-filters-message-help = Pattern glob confrontati con i titoli degli eventi. Usa <code class="text-mono">*</code> per una sequenza qualsiasi, <code class="text-mono">?</code> per un singolo carattere.
projects-filters-col-pattern = Pattern
projects-filters-message-empty = Nessun filtro messaggi configurato.
projects-filters-add-pattern = Aggiungi pattern
projects-filters-message-submit = Aggiungi filtro messaggi
projects-filters-ratelimit-heading = Limite di frequenza
projects-filters-ratelimit-desc = Numero massimo di eventi al minuto per questo progetto. 0 = illimitato.
projects-filters-ratelimit-label = Eventi al minuto
projects-filters-ratelimit-submit = Salva limite di frequenza
projects-filters-env-heading = Ambienti esclusi
projects-filters-env-desc = Gli eventi provenienti da questi ambienti verranno scartati silenziosamente.
projects-filters-col-environment = Ambiente
projects-filters-env-empty = Nessun ambiente escluso.
projects-filters-env-add-label = Aggiungi ambiente escluso
projects-filters-env-submit = Escludi ambiente
projects-filters-release-heading = Filtri release
projects-filters-release-desc = Pattern glob confrontati con le versioni delle release. Gli eventi corrispondenti vengono scartati.
projects-filters-release-empty = Nessun filtro release.
projects-filters-release-submit = Aggiungi filtro release
projects-filters-ua-heading = Filtri User-Agent
projects-filters-ua-desc = Pattern glob confrontati con gli header User-Agent. I pattern integrati per kube-probe e i controllori di integrità sono sempre attivi.
projects-filters-ua-empty = Nessun filtro User-Agent personalizzato.
projects-filters-ua-submit = Aggiungi filtro User-Agent
projects-filters-rules-heading = Regole personalizzate
projects-filters-rules-desc = Regole avanzate che confrontano i campi degli eventi. Le regole con priorità più alta vengono valutate per prime.
projects-filters-col-field = Campo
projects-filters-col-operator = Operatore
projects-filters-col-value = Valore
projects-filters-col-action = Azione
projects-filters-col-priority = Priorità
projects-filters-rules-empty = Nessuna regola personalizzata.
projects-filters-sample-rate-label = Frequenza di campionamento
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Aggiungi regola
projects-filters-op = { $op ->
    [not_equals] diverso da
    [contains] contiene
    [not_contains] non contiene
    [starts_with] inizia con
    [in] incluso in
    [not_in] non incluso in
   *[equals] uguale a
}
projects-filters-action = { $action ->
    [sample] campiona
   *[drop] scarta
}
projects-filters-ip-heading = Lista di blocco IP
projects-filters-ip-desc = Blocchi CIDR o singoli IP. Gli eventi provenienti da IP bloccati vengono scartati silenziosamente.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = Nessun blocco IP configurato.
projects-filters-ip-add-label = Aggiungi CIDR
projects-filters-ip-submit = Blocca intervallo IP
projects-filters-discard-heading = Statistiche di scarto
projects-filters-discard-window = (ultimi 7 giorni)
projects-filters-col-date = Data
projects-filters-col-reason = Motivo
projects-filters-col-count = Conteggio

# Etichette delle entità filtro, interpolate in flash-not-found-filter all'eliminazione.
projects-filter-label-message = filtro messaggi
projects-filter-label-environment = filtro ambiente
projects-filter-label-release = filtro release
projects-filter-label-user-agent = filtro User-Agent
projects-filter-label-rule = regola di filtro

# --- Impostazioni: integrazioni ---
projects-integrations-active-heading = Integrazioni attive
projects-integrations-active-empty = Nessuna integrazione attivata. Aggiungi prima un'integrazione globale nella pagina <a class="text-primary" href="/web/settings/integrations/">Integrazioni</a>, poi abilitala qui. Puoi delimitare ciascuna per livello minimo e ambiente, così il rumore di dev resta fuori dai canali di prod.
projects-integrations-deactivate-confirm = Disattivare questa integrazione per il progetto?
projects-integrations-deactivate = Disattiva
projects-integrations-notify-new-issues = Nuovi problemi
projects-integrations-notify-regressions = Regressioni
projects-integrations-notify-threshold = Avvisi di soglia
projects-integrations-notify-digests = Riepiloghi
projects-integrations-min-level = Livello minimo
projects-integrations-level-any = Qualsiasi
projects-integrations-env-filter = Filtro ambiente
projects-integrations-env-placeholder = es. production
projects-integrations-to-address = Indirizzo destinatario
projects-integrations-to-address-note = (solo integrazioni email)
projects-integrations-activate-heading = Attiva integrazione
projects-integrations-integration-label = Integrazione
projects-integrations-activate-submit = Attiva
projects-integrations-available-empty = Nessuna integrazione disponibile. <a class="text-primary" href="/web/settings/integrations/">Creane prima una</a>.

# --- Progetto creato ---
projects-created-word = creato
projects-created-breadcrumb = Creato
projects-created-heading = Progetto creato
projects-created-subtitle = Usa il DSN qui sotto per configurare il tuo SDK.
projects-created-settings-btn = Impostazioni progetto
projects-created-back = Torna ai progetti
projects-created-details-heading = Dettagli del progetto
projects-created-col-id = ID progetto
projects-created-sdk-desc-before = Installa l'SDK Sentry per
projects-created-sdk-desc-after = e inizializzalo con il DSN qui sopra.
projects-created-docs-javascript = Documentazione Sentry JavaScript →
projects-created-docs-python = Documentazione Sentry Python →
projects-created-docs-rust = Documentazione Sentry Rust →
projects-created-docs-go = Documentazione Sentry Go →
projects-created-docs-node = Documentazione Sentry Node.js →
projects-created-docs-java = Documentazione Sentry Java →
projects-created-docs-ruby = Documentazione Sentry Ruby →
projects-created-docs-php = Documentazione Sentry PHP →
projects-created-docs-elixir = Documentazione Sentry Elixir →
projects-created-docs-dotnet = Documentazione Sentry .NET →
projects-created-docs-apple = Documentazione Sentry Apple →
projects-created-docs-kotlin = Documentazione Sentry Kotlin →
projects-created-docs-native = Documentazione Sentry Native →
projects-created-docs-generic = Documentazione della piattaforma Sentry →
