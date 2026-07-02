# Interface des journaux : la liste des journaux par projet. Réutilise nav-logs.
# Les chaînes comptées utilisent les pluriels tv_count ([one]/[other]).

# --- Suffixe de titre de page ---
logs-title-suffix = — Stackpit

# --- Liste des journaux ---
logs-list-search-placeholder = Rechercher dans les journaux…
logs-list-search-label = Rechercher dans les journaux
logs-list-filter-level = Filtrer par niveau
logs-list-level-all = Tous les niveaux
logs-filter-submit = Filtrer
logs-list-empty = Aucun journal ne correspond aux filtres actuels.
logs-col-timestamp = Horodatage
logs-col-level = Niveau
logs-col-body = Corps
logs-col-trace = Trace
logs-col-release = Release
logs-body-empty = (vide)

# --- Pagination ---
logs-pagination-label = Pagination
logs-pagination-prev = « Précédent
logs-pagination-next = Suivant »
logs-count = { $count ->
    [one] { $count } journal
   *[other] { $count } journaux
}
