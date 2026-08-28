# Interface des transactions : la liste des transactions par projet et la page
# de détail (instances). Réutilise nav-transactions pour l'en-tête/le fil
# d'Ariane/le titre. Les chaînes comptées utilisent les pluriels tv_count.

# --- Suffixe de titre de page (titres à préfixe dynamique) ---
transactions-title-suffix = — Stackpit

# --- Liste des transactions ---
transactions-time-range = Plage temporelle
transactions-filter-submit = Filtrer
transactions-list-empty = Aucune transaction sur cette période.
transactions-col-name = Transaction
transactions-col-throughput = Débit
transactions-col-failure = % d'échecs
transactions-col-count = Nombre
transactions-col-users = Utilisateurs

# --- Détail d'une transaction (instances) ---
transactions-detail-op = op:
transactions-detail-empty = Aucune instance enregistrée pour cette transaction.
transactions-detail-col-duration = Durée
transactions-detail-col-status = Statut
transactions-detail-col-trace = Trace
transactions-detail-col-when = Quand
transactions-detail-distribution = Distribution des durées
transactions-detail-spans = Répartition des spans
transactions-detail-issues = Problèmes associés
transactions-detail-instances = Instances les plus lentes
transactions-detail-trend = Tendance des centiles
transactions-detail-trend-note = Les points marqués sont ceux où le p95 a dépassé 1,5 fois la médiane des cinq points précédents.

# --- Pagination (détail d'une transaction) ---
transactions-pagination-label = Pagination
transactions-pagination-prev = « Précédent
transactions-pagination-next = Suivant »
transactions-detail-count = { $count ->
    [one] { $count } instance
   *[other] { $count } instances
}
transactions-detail-failure-label = Échecs
