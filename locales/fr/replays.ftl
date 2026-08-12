# Interface des replays : la liste des replays par projet et la page de détail.
# Réutilise nav-replays. Les chaînes comptées utilisent les pluriels tv_count
# ([one]/[other]).

# --- Suffixe de titre de page ---
replays-title-suffix = — Stackpit

# --- Liste des replays ---
replays-list-empty = Aucun replay trouvé. Les événements de replay apparaîtront ici.
replays-col-event-id = ID de l'événement
replays-col-type = Type
replays-col-release = Release
replays-col-url = URL
replays-col-user = Utilisateur
replays-col-browser = Navigateur
replays-col-duration = Durée
replays-col-errors = Erreurs
replays-col-environment = Environnement
replays-col-timestamp = Horodatage

# --- Détail d'un replay ---
replays-detail-heading = Replay
replays-detail-note = La lecture de l'enregistrement n'est pas encore disponible. Les données brutes du replay sont affichées ci-dessous.
replays-detail-raw-payload = Données brutes
replays-related-errors = Erreurs dans ce replay
replays-col-level = Niveau
replays-col-title = Titre

# --- Pagination ---
replays-pagination-label = Pagination
replays-pagination-prev = « Précédent
replays-pagination-next = Suivant »
replays-count = { $count ->
    [one] { $count } replay
   *[other] { $count } replays
}
