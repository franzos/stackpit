# Superficie span: l'elenco span/trace per progetto (spans-*) e la pagina di
# dettaglio del trace a cascata (trace-detail-*). Usa nav-spans. Le stringhe
# conteggiate usano i plurali tv_count ([one]/[other]).

# --- Suffisso del titolo ---
spans-title-suffix = — Stackpit

# --- Elenco span/trace ---
spans-list-empty = Nessuno span trovato per questo progetto.
spans-traces-heading = Trace
spans-all-heading = Tutti gli span

# --- Tabella trace ---
spans-col-trace-id = ID trace
spans-col-root-op = Op radice
spans-col-root-description = Descrizione radice
spans-col-duration = Durata
spans-col-first-seen = Prima occorrenza
spans-col-last-seen = Ultima occorrenza

# --- Tabella tutti gli span ---
spans-col-span-id = ID span
spans-col-op = Op
spans-col-description = Descrizione
spans-col-timestamp = Timestamp

# --- Impaginazione (elenco span) ---
spans-pagination-label = Impaginazione
spans-pagination-prev = « Precedente
spans-pagination-next = Successivo »
spans-count = { $count ->
    [one] { $count } span
   *[other] { $count } span
}

# --- Dettaglio trace (cascata) ---
# title-prefix/suffix avvolgono l'id dinamico del trace; total/showing-first/of
# sono divisi ai confini { $var } della riga meta.
trace-detail-title-prefix = Trace
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = totale
trace-detail-showing-first = mostrando i primi
trace-detail-of = di
trace-detail-empty = Nessuno span trovato per questo trace.
trace-detail-col-span = Span
trace-detail-col-duration = Durata
trace-detail-root-fallback = (radice del trace)
trace-detail-error-title = errore
trace-detail-span-fallback = span
trace-detail-compressed-note = intervalli inattivi compressi
trace-detail-gap-title = Intervallo inattivo compresso (nessuno span attivo)
trace-detail-lbl-span-id = ID span
trace-detail-lbl-parent = Span padre
trace-detail-lbl-status = Stato
trace-detail-lbl-start = Offset di inizio
trace-detail-correlated-errors = Errori correlati
trace-detail-col-level = Livello
trace-detail-col-title = Titolo
trace-detail-col-timestamp = Timestamp
trace-detail-span-count = { $count ->
    [one] { $count } span
   *[other] { $count } span
}
