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
release-health-col-crash-free-users = Utenti senza crash
release-health-na = n/d
