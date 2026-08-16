# Superficie problemi: l'elenco raggruppato per impronta e la pagina di dettaglio.
# issue-detail-exception-stacktrace contiene un &amp; inline ed è renderizzato con
# |safe. Le stringhe conteggiate usano i plurali tv_count ([one]/[other]).

# --- Etichette condivise (elenco problemi + dettaglio problema) ---
issues-label-title = Titolo
issues-label-level = Livello
issues-label-events = Eventi
issues-label-users = Utenti
issues-label-trend = Andamento
issues-trend-tooltip = Volume di eventi nel periodo selezionato
issues-label-status = Stato
issues-label-first-seen = Prima occorrenza
issues-label-last-seen = Ultima occorrenza
issues-label-value = Valore

# --- Valori di stato (opzioni di filtro + badge) ---
issues-status-unresolved = Non risolto
issues-status-resolved = Risolto
issues-status-ignored = Ignorato

# --- Impaginazione (condivisa) ---
issues-pagination-label = Impaginazione
issues-pagination-prev = « Precedente
issues-pagination-next = Successivo »

# --- Suffisso del titolo (titoli con prefisso dinamico) ---
issues-title-suffix = — Stackpit

# --- Elenco problemi ---
issues-list-subtitle = Problemi raggruppati per impronta.
issues-list-filtered-by-tag = Filtrato per tag:
issues-list-clear-tag = Rimuovi filtro tag
issues-list-search-placeholder = Cerca problemi…
issues-list-search-label = Cerca problemi
issues-list-select = Seleziona problema
issues-list-filter-status = Filtra per stato
issues-list-status-all = Tutti gli stati
issues-list-filter-level = Filtra per livello
issues-list-level-all = Tutti i livelli
issues-list-filter-release = Filtra per release
issues-list-release-all = Tutte le release
issues-list-filter-environment = Filtra per ambiente
issues-list-environment-all = Tutti gli ambienti
issues-period-label = Intervallo di tempo
issues-list-filter-submit = Filtra
issues-list-empty = Nessun problema corrisponde ai filtri attuali.
issues-untitled = (senza titolo)

# --- Azioni in blocco ---
issues-bulk-resolve-all = Risolvi tutti i { $count }
issues-bulk-ignore-all = Ignora tutti i { $count }
issues-bulk-delete-all = Elimina tutti i { $count }
issues-bulk-resolve-confirm = { $count ->
    [one] Risolvere { $count } problema corrispondente?
   *[other] Risolvere tutti i { $count } problemi corrispondenti?
}
issues-bulk-ignore-confirm = { $count ->
    [one] Ignorare { $count } problema corrispondente?
   *[other] Ignorare tutti i { $count } problemi corrispondenti?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] Eliminare definitivamente { $count } problema corrispondente?
   *[other] Eliminare definitivamente tutti i { $count } problemi corrispondenti?
}
issues-bulk-resolve = Risolvi
issues-bulk-ignore = Ignora
issues-bulk-delete = Elimina
issues-bulk-delete-selected-confirm = Eliminare definitivamente i problemi selezionati?

# --- Conteggio (impaginazione) ---
issues-count = { $count ->
    [one] { $count } problema
   *[other] { $count } problemi
}

# --- Dettaglio problema ---
issue-detail-title-fallback = Problema
issue-detail-resolve = ✓ Risolvi
issue-detail-reopen = Riapri
issue-detail-unignore = Non ignorare più
issue-detail-tab-details = Dettagli
issue-detail-tab-events = Tutti gli eventi
issue-detail-exception-stacktrace = Eccezione &amp; Stacktrace
issue-detail-handled = gestita
issue-detail-unhandled = non gestita
issue-detail-in = in
issue-detail-var-name = Variabile
issue-detail-no-source = Nessun contesto del codice sorgente disponibile
issue-detail-in-app-only = Solo frame dell'app
issue-detail-reverse-order = Inverti l'ordine
issue-detail-copy = Copia
issue-detail-copy-frame = Copia questo frame
issue-detail-library-frames = { $count ->
    [one] { $count } frame di libreria
   *[other] { $count } frame di libreria
}
issue-detail-minified-hint = Questi frame sembrano minificati e non è stata applicata alcuna source map.
issue-detail-minified-hint-link = Carica source map
issue-detail-breadcrumbs = Breadcrumb
issue-detail-th-time = Ora
issue-detail-th-category = Categoria
issue-detail-th-message = Messaggio
issue-detail-crumb-data = dati
issue-detail-crumb-filter = Filtra i breadcrumb per tipo
issue-detail-crumb-filter-all = Tutti i tipi
issue-detail-tags = Tag
issue-detail-contexts = Contesti
issue-detail-additional-data = Dati aggiuntivi
issue-detail-view-replay = Vedi replay
issue-detail-view-trace = Vedi traccia
issue-detail-request = Richiesta
issue-detail-headers = Header
issue-detail-th-header = Header
issue-detail-query-string = Query string
issue-detail-body = Corpo
issue-detail-environment = Ambiente
issue-detail-user-reports = Report utente
issue-detail-anonymous = Anonimo
issue-detail-attachments = Allegati
issue-detail-att-filename = Nome file
issue-detail-att-type = Tipo
issue-detail-att-size = Dimensione
issue-detail-download = Scarica
issue-detail-raw-json = JSON grezzo
issue-detail-no-events = Nessun evento trovato per questo problema.
issue-detail-ev-id = ID evento
issue-detail-ev-timestamp = Timestamp
issue-detail-ev-platform = Piattaforma
issue-detail-events-count = { $count ->
    [one] { $count } evento
   *[other] { $count } eventi
}
issue-detail-props-heading = Proprietà del problema
issue-detail-fingerprint = Impronta
issue-detail-tag-facets = Facet dei tag
issue-detail-discard-undo-title = Riprendi ad accettare gli eventi futuri con questa impronta
issue-detail-discard-undo = Annulla lo scarto
issue-detail-discard-confirm = Scartare tutti gli eventi futuri con questa impronta?
issue-detail-discard-title = Scarta silenziosamente gli eventi futuri con questa impronta
issue-detail-discard = Scarta eventi futuri
issue-detail-create-external-issue = Crea issue
issue-detail-external-tracker = Tracker esterno
issue-detail-view-on = Vedi su
flash-tracker-create-failed = Impossibile creare la issue. Controlla token e repository dell'integrazione e riprova.
flash-tracker-config-incomplete = A questa integrazione manca un repository o un token. Correggilo nelle impostazioni dell'integrazione.
issue-detail-external-unlink = Scollega
issue-detail-external-unlink-confirm = Rimuovere questo collegamento? La issue resta sul forge: chiudila o eliminala lì.
issue-detail-external-orphaned = integrazione rimossa
flash-tracker-unlinked = Collegamento rimosso. La issue esiste ancora sul forge.
flash-tracker-ambiguous = Questo progetto ha più di un repository in cui questo tracker può creare la issue. Scegline uno e riprova.
