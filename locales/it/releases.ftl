# Superficie release: l'elenco release tra progetti e la pagina dello stato delle
# release per progetto. Usa nav-releases e nav-health. Le stringhe conteggiate
# usano i plurali tv_count ([one]/[other]).

# --- Suffisso del titolo ---
releases-title-suffix = — Stackpit

# --- Elenco release ---
releases-list-search-placeholder = Cerca release…
releases-list-search-label = Cerca release
releases-list-project-placeholder = ID progetto
releases-list-project-label = Filtra per progetto
releases-list-period-label = Periodo di adozione
releases-list-period-24h = Ultime 24h
releases-list-period-7d = Ultimi 7 giorni
releases-list-period-30d = Ultimi 30 giorni
releases-filter-submit = Filtra
releases-list-empty = Ancora nessuna release. Imposta un <code class="text-mono">release</code> nel tuo SDK e appariranno qui non appena arriveranno eventi.
releases-col-version = Versione
releases-col-project = Progetto
releases-col-issues = Problemi
releases-col-events = Eventi
releases-col-adoption = Adozione
releases-col-first-seen = Prima occorrenza
releases-col-last-seen = Ultima occorrenza

# --- Impaginazione ---
releases-pagination-label = Impaginazione
releases-pagination-prev = « Precedente
releases-pagination-next = Successivo »
releases-count = { $count ->
    [one] { $count } release
   *[other] { $count } release
}

# --- Stato delle release ---
release-health-title = Stato delle release
release-health-heading = Stato delle release
release-health-sessions-heading = Sessioni nel tempo
release-health-empty = Nessun dato di sessione disponibile. Gli eventi di sessione con un campo <code class="text-mono">status</code> appariranno qui.
release-health-col-release = Release
release-health-col-sessions = Sessioni
release-health-col-ok = OK
release-health-col-crashed = In crash
release-health-col-errored = Con errori
release-health-col-crash-free-sessions = Sessioni senza crash
release-health-col-error-free-sessions = Sessioni senza errori
release-health-col-crash-free-users = Utenti senza crash
release-health-subtitle = Gli esiti delle sessioni sono segnali di stato riportati dall'SDK, non eventi di errore. Fai clic su una release per vederne i problemi.
release-health-crashed-title = Vedi i problemi di questa release
release-health-errored-title = Vedi i problemi di questa release
release-health-errored-hint = Il conteggio «con errori» sono segnali di stato di sessione riportati dall'SDK (una sessione che ha registrato un errore gestito ma non è andata in crash), non singoli eventi di errore, e non può essere elencato per sessione. I problemi collegati sono i gruppi di errori visti in questa release.

# --- Dettaglio release (per versione) ---
release-detail-sessions-heading = Stato delle sessioni
release-detail-sessions-note = Esiti delle sessioni riportati dall'SDK (ok / con errori / in crash). Sono segnali di stato, non singoli eventi di errore.
release-detail-no-health = Nessun dato di sessione per questa release.
release-detail-issues-heading = Problemi in questa release
release-detail-issues-note = Gruppi di errori distinti visti per la prima o l'ultima volta con questa release.
release-detail-no-issues = Nessun problema registrato per questa release.
release-health-na = n/d
