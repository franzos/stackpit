# Interface des métriques : la liste des métriques par projet et la page de
# détail de la série. Réutilise nav-metrics. Les chaînes comptées utilisent les
# pluriels tv_count ([one]/[other]).

# --- Suffixe de titre de page ---
metrics-title-suffix = — Stackpit

# --- Liste des métriques ---
metrics-list-empty = Aucune métrique trouvée. Les événements de métrique apparaîtront ici une fois reçus.
metrics-col-mri = MRI
metrics-col-type = Type
metrics-col-data-points = Points de données
metrics-col-first-seen = Première apparition
metrics-col-last-seen = Dernière apparition

# --- Pagination ---
metrics-pagination-label = Pagination
metrics-pagination-prev = « Précédent
metrics-pagination-next = Suivant »
metrics-count = { $count ->
    [one] { $count } métrique
   *[other] { $count } métriques
}

# --- Détail de la métrique (tranches horaires) ---
metrics-detail-empty = Aucun point de données dans la plage temporelle sélectionnée.
metrics-detail-col-time = Heure (tranche horaire)
metrics-detail-col-count = Nombre
metrics-detail-col-sum = Somme
metrics-detail-col-min = Min
metrics-detail-col-max = Max
metrics-detail-col-avg = Moy.
