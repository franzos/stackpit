# Interface des releases : la liste inter-projets et la page de santé des
# releases par projet. Réutilise nav-releases et nav-health. Les chaînes
# comptées utilisent les pluriels tv_count ([one]/[other]).

# --- Suffixe de titre de page ---
releases-title-suffix = — Stackpit

# --- Liste des releases ---
releases-list-search-placeholder = Rechercher des releases…
releases-list-search-label = Rechercher des releases
releases-list-project-placeholder = ID du projet
releases-list-project-label = Filtrer par projet
releases-list-period-label = Période d'adoption
releases-list-period-24h = Dernières 24 h
releases-list-period-7d = 7 derniers jours
releases-list-period-30d = 30 derniers jours
releases-filter-submit = Filtrer
releases-list-empty = Aucune release pour le moment. Définissez un <code class="text-mono">release</code> dans votre SDK et elles apparaîtront ici dès que des événements arriveront.
releases-col-version = Version
releases-col-project = Projet
releases-col-issues = Problèmes
releases-col-events = Événements
releases-col-adoption = Adoption
releases-col-first-seen = Première apparition
releases-col-last-seen = Dernière apparition

# --- Pagination ---
releases-pagination-label = Pagination
releases-pagination-prev = « Précédent
releases-pagination-next = Suivant »
releases-count = { $count ->
    [one] { $count } release
   *[other] { $count } releases
}

# --- Santé des releases ---
release-health-title = Santé des releases
release-health-heading = Santé des releases
release-health-sessions-heading = Sessions au fil du temps
release-health-period-label = Période
release-health-period-1h = Dernière heure
release-health-period-24h = Dernières 24 h
release-health-period-7d = 7 derniers jours
release-health-period-14d = 14 derniers jours
release-health-period-30d = 30 derniers jours
release-health-period-90d = 90 derniers jours
release-health-empty = Aucune donnée de session disponible. Les événements de session avec un champ <code class="text-mono">status</code> apparaîtront ici.
release-health-col-release = Release
release-health-col-sessions = Sessions
release-health-col-ok = OK
release-health-col-crashed = Plantées
release-health-col-errored = En erreur
release-health-col-crash-free-sessions = Sessions sans plantage
release-health-col-error-free-sessions = Sessions sans erreur
release-health-col-crash-free-users = Utilisateurs sans plantage
release-health-subtitle = Les résultats de session sont des signaux de santé rapportés par le SDK, pas des événements d'erreur. Cliquez sur une release pour voir ses problèmes.
release-health-crashed-title = Voir les problèmes de cette release
release-health-errored-title = Voir les problèmes de cette release
release-health-errored-hint = Le compte « en erreur » correspond à des signaux de santé de session rapportés par le SDK (une session ayant enregistré une erreur gérée sans planter), pas à des événements d'erreur individuels, et ne peut pas être listé par session. Les problèmes liés sont les groupes d'erreurs vus dans cette release.

# --- Détail d'une release (par version) ---
release-detail-sessions-heading = Santé des sessions
release-detail-sessions-note = Résultats de session rapportés par le SDK (ok / en erreur / plantées). Ce sont des signaux de santé, pas des événements d'erreur individuels.
release-detail-no-health = Aucune donnée de session pour cette release.
release-detail-issues-heading = Problèmes de cette release
release-detail-issues-note = Groupes d'erreurs distincts vus pour la première ou la dernière fois avec cette release.
release-detail-no-issues = Aucun problème enregistré pour cette release.
release-health-na = n/a
