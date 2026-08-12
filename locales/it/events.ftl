# Superficie eventi: l'elenco eventi tra progetti e la pagina di dettaglio evento.
# event-detail-exception-stacktrace contiene un &amp; inline ed è renderizzato con
# |safe. Le stringhe conteggiate usano i plurali tv_count ([one]/[other]).

# --- Etichette condivise (elenco eventi + dettaglio evento) ---
events-label-title = Titolo
events-label-type = Tipo
events-label-level = Livello
events-label-platform = Piattaforma
events-label-environment = Ambiente
events-label-time = Ora
events-label-value = Valore

# --- Impaginazione (condivisa) ---
events-pagination-label = Impaginazione
events-pagination-prev = « Precedente
events-pagination-next = Successivo »

# --- Suffisso del titolo (titoli con prefisso dinamico) ---
events-title-suffix = — Stackpit

# --- Elenco eventi ---
events-list-title = Eventi — Stackpit
events-heading = Eventi
events-list-search-placeholder = Cerca eventi…
events-list-search-label = Cerca eventi
events-list-select = Seleziona evento
events-list-filter-level = Filtra per livello
events-list-level-all = Tutti i livelli
events-list-filter-type = Filtra per tipo
events-list-type-all = Tutti i tipi
events-list-project-placeholder = ID progetto
events-list-filter-project = Filtra per progetto
events-list-filter-submit = Filtra
events-list-empty = Nessun evento corrisponde ai filtri attuali.
events-untitled = (senza titolo)
events-col-project = Progetto

# --- Azioni in blocco ---
events-bulk-delete = Elimina
events-bulk-delete-selected-confirm = Eliminare gli eventi selezionati?
events-bulk-delete-all = Elimina tutti i { $count } corrispondenti
events-bulk-delete-all-confirm = { $count ->
    [one] Eliminare definitivamente { $count } evento corrispondente?
   *[other] Eliminare definitivamente tutti i { $count } eventi corrispondenti?
}

# --- Conteggio (impaginazione) ---
events-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventi
}

# --- Dettaglio evento ---
event-detail-event = Evento
event-detail-event-id-label = event_id:
event-detail-nav-label = Navigazione eventi
event-detail-nav-newer = « Più recente
event-detail-nav-older = Più vecchio »
event-detail-nav-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventi
}
event-detail-nav-in-issue = nel problema
event-detail-user-feedback = Feedback utente
event-detail-anonymous = Anonimo
event-detail-related-event = Evento correlato:
event-detail-exception-stacktrace = Eccezione &amp; Stacktrace
event-detail-handled = gestita
event-detail-unhandled = non gestita
event-detail-in = in
event-detail-var-name = Variabile
event-detail-no-source = Nessun contesto del codice sorgente disponibile
event-detail-breadcrumbs = Breadcrumb
event-detail-th-category = Categoria
event-detail-th-message = Messaggio
event-detail-tags = Tag
event-detail-contexts = Contesti
event-detail-request = Richiesta
event-detail-headers = Header
event-detail-th-header = Header
event-detail-query-string = Query string
event-detail-body = Corpo
event-detail-user-reports = Report utente
event-detail-attachments = Allegati
event-detail-att-filename = Nome file
event-detail-att-size = Dimensione
event-detail-download = Scarica
event-detail-web-vitals = Web Vitals
event-detail-raw-json = JSON grezzo
event-detail-props-heading = Proprietà dell'evento
event-detail-prop-event-id = ID evento
event-detail-prop-timestamp = Timestamp
event-detail-prop-transaction = Transazione
event-detail-prop-release = Release
event-detail-prop-server = Server
event-detail-prop-sdk = SDK
event-detail-prop-received = Ricevuto
event-detail-user-heading = Utente
event-detail-user-id = ID
event-detail-user-email = Email
event-detail-user-username = Nome utente
event-detail-user-ip = Indirizzo IP

# --- Report client (eventi scartati) ---
# Usa events-untitled ed events-pagination-* (condivisi, stessa file).
client-reports-title = Report client
client-reports-dropped-heading = Eventi scartati
client-reports-dropped-subtitle = Ciò che gli SDK hanno scartato prima dell'invio, per categoria e motivo.
client-reports-th-category = Categoria
client-reports-th-reason = Motivo
client-reports-th-reasons = Motivi
client-reports-th-dropped = Scartati
client-reports-empty = Nessun report client trovato per questo progetto.
client-reports-reports-heading = Report
client-reports-delete = Elimina
client-reports-delete-selected-confirm = Eliminare i report selezionati?
client-reports-th-event-id = ID evento
client-reports-th-title = Titolo
client-reports-th-timestamp = Timestamp
client-reports-th-platform = Piattaforma
client-reports-th-release = Release
client-reports-select = Seleziona report
client-reports-delete-all = Elimina tutti i { $count }
client-reports-delete-all-confirm = { $count ->
    [one] Eliminare { $count } report corrispondente?
   *[other] Eliminare tutti i { $count } report corrispondenti?
}
client-reports-count = { $count ->
    [one] { $count } report
   *[other] { $count } report
}

# --- Report utente (feedback degli utenti) ---
# Usa events-untitled ed events-pagination-* (condivisi, stessa file).
user-reports-title = Report utente
user-reports-heading = Report utente
user-reports-empty = Nessun report utente trovato per questo progetto.
user-reports-delete = Elimina
user-reports-delete-selected-confirm = Eliminare i report selezionati?
user-reports-th-event-id = ID evento
user-reports-th-title = Titolo
user-reports-th-timestamp = Timestamp
user-reports-th-platform = Piattaforma
user-reports-th-release = Release
user-reports-select = Seleziona report
user-reports-delete-all = Elimina tutti i { $count }
user-reports-delete-all-confirm = { $count ->
    [one] Eliminare { $count } report corrispondente?
   *[other] Eliminare tutti i { $count } report corrispondenti?
}
user-reports-count = { $count ->
    [one] { $count } report
   *[other] { $count } report
}
