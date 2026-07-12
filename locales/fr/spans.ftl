# Interface des spans : la liste des spans/traces par projet (spans-*) et la
# page de détail en cascade d'une trace (trace-detail-*). Réutilise nav-spans.
# Les chaînes comptées utilisent les pluriels tv_count ([one]/[other]).

# --- Suffixe de titre de page ---
spans-title-suffix = — Stackpit

# --- Liste des spans/traces ---
spans-list-empty = Aucun span trouvé pour ce projet.
spans-traces-heading = Traces
spans-all-heading = Tous les spans

# --- Tableau des traces ---
spans-col-trace-id = ID de trace
spans-col-root-op = Op racine
spans-col-root-description = Description racine
spans-col-duration = Durée
spans-col-first-seen = Première apparition
spans-col-last-seen = Dernière apparition

# --- Tableau de tous les spans ---
spans-col-span-id = ID de span
spans-col-op = Op
spans-col-description = Description
spans-col-timestamp = Horodatage

# --- Pagination (liste des spans) ---
spans-pagination-label = Pagination
spans-pagination-prev = « Précédent
spans-pagination-next = Suivant »
spans-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}

# --- Détail d'une trace (cascade) ---
# title-prefix/suffix encadrent l'id de trace dynamique ; total/showing-first/of
# sont découpés aux limites { $var } de la ligne de méta.
trace-detail-title-prefix = Trace
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = au total
trace-detail-showing-first = affichage des premiers
trace-detail-of = sur
trace-detail-empty = Aucun span trouvé pour cette trace.
trace-detail-col-span = Span
trace-detail-col-duration = Durée
trace-detail-root-fallback = (racine de la trace)
trace-detail-error-title = erreur
trace-detail-span-fallback = span
trace-detail-compressed-note = intervalles inactifs compressés
trace-detail-gap-title = Intervalle inactif réduit (aucun span actif)
trace-detail-lbl-span-id = ID du span
trace-detail-lbl-parent = Span parent
trace-detail-lbl-status = Statut
trace-detail-lbl-start = Décalage de début
trace-detail-correlated-errors = Erreurs corrélées
trace-detail-col-level = Niveau
trace-detail-col-title = Titre
trace-detail-col-timestamp = Horodatage
trace-detail-span-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}
