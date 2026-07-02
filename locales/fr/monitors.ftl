# Interface des moniteurs : la liste des moniteurs (check-ins cron) par projet
# et la page de détail. Réutilise nav-monitors. Les chaînes comptées utilisent
# les pluriels tv_count ([one]/[other]).

# --- Suffixe de titre de page ---
monitors-title-suffix = — Stackpit

# --- Liste des moniteurs ---
monitors-list-empty = Aucun moniteur trouvé. Les événements de check-in avec un <code class="text-mono">monitor_slug</code> apparaîtront ici.
monitors-col-slug = Slug
monitors-col-last-status = Dernier statut
monitors-col-last-checkin = Dernier check-in
monitors-col-count = Nombre

# --- Détail d'un moniteur ---
monitors-detail-title-prefix = Moniteur
monitors-detail-subtitle = Check-ins du moniteur.
monitors-detail-empty = Aucun check-in trouvé pour ce moniteur.
monitors-detail-select-checkin = Sélectionner le check-in
monitors-detail-confirm-delete-selected = Supprimer les check-ins sélectionnés ?
monitors-detail-delete = Supprimer
monitors-detail-col-title = Titre
monitors-detail-col-level = Niveau
monitors-detail-col-environment = Environnement
monitors-detail-col-time = Heure
monitors-detail-untitled = (sans titre)
monitors-detail-confirm-delete-all = { $count ->
    [one] Supprimer les { $count } check-ins ?
   *[other] Supprimer les { $count } check-ins ?
}
monitors-detail-delete-all = { $count ->
    [one] Supprimer les { $count }
   *[other] Supprimer les { $count }
}

# --- Pagination ---
monitors-pagination-label = Pagination
monitors-pagination-prev = « Précédent
monitors-pagination-next = Suivant »
monitors-detail-count = { $count ->
    [one] { $count } check-in
   *[other] { $count } check-ins
}
