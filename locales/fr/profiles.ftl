# Interface des profils : la liste des profils par projet et la page de détail.
# Réutilise nav-profiles. Les chaînes comptées utilisent les pluriels tv_count
# ([one]/[other]).

# --- Suffixe de titre de page ---
profiles-title-suffix = — Stackpit

# --- Liste des profils ---
profiles-list-empty = Aucun profil trouvé. Les événements de profil avec <code class="text-mono">item_type = "profile"</code> apparaîtront ici.
profiles-col-event-id = ID de l'événement
profiles-col-transaction = Transaction
profiles-col-platform = Plateforme
profiles-col-release = Release
profiles-col-environment = Environnement
profiles-col-timestamp = Horodatage

# --- Détail d'un profil ---
profiles-detail-heading = Profil
profiles-detail-raw-payload = Données brutes

# --- Pagination ---
profiles-pagination-label = Pagination
profiles-pagination-prev = « Précédent
profiles-pagination-next = Suivant »
profiles-count = { $count ->
    [one] { $count } profil
   *[other] { $count } profils
}
