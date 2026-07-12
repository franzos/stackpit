# Interface des problèmes : la liste groupée par empreinte et la page de détail
# d'un problème. issue-detail-exception-stacktrace contient un &amp; inline et est
# rendu avec |safe. Les chaînes comptées utilisent les pluriels tv_count.

# --- Étiquettes partagées (liste + détail) ---
issues-label-title = Titre
issues-label-level = Niveau
issues-label-events = Événements
issues-label-users = Utilisateurs
issues-label-status = Statut
issues-label-first-seen = Première apparition
issues-label-last-seen = Dernière apparition
issues-label-value = Valeur

# --- Valeurs de statut (options de filtre + badges) ---
issues-status-unresolved = Non résolu
issues-status-resolved = Résolu
issues-status-ignored = Ignoré

# --- Pagination (partagée) ---
issues-pagination-label = Pagination
issues-pagination-prev = « Précédent
issues-pagination-next = Suivant »

# --- Suffixe de titre de page (titres à préfixe dynamique) ---
issues-title-suffix = — Stackpit

# --- Liste des problèmes ---
issues-list-subtitle = Problèmes groupés par empreinte.
issues-list-filtered-by-tag = Filtré par tag :
issues-list-clear-tag = Effacer le filtre de tag
issues-list-search-placeholder = Rechercher des problèmes…
issues-list-search-label = Rechercher des problèmes
issues-list-select = Sélectionner le problème
issues-list-filter-status = Filtrer par statut
issues-list-status-all = Tous les statuts
issues-list-filter-level = Filtrer par niveau
issues-list-level-all = Tous les niveaux
issues-list-filter-release = Filtrer par release
issues-list-release-all = Toutes les releases
issues-period-label = Plage temporelle
issues-period-all = Tout l'historique
issues-period-1h = Dernière heure
issues-period-24h = Dernières 24 h
issues-period-7d = 7 derniers jours
issues-period-14d = 14 derniers jours
issues-period-30d = 30 derniers jours
issues-period-90d = 90 derniers jours
issues-period-365d = 365 derniers jours
issues-list-filter-submit = Filtrer
issues-list-empty = Aucun problème ne correspond aux filtres actuels.
issues-untitled = (sans titre)

# --- Actions groupées ---
issues-bulk-resolve-all = Résoudre les { $count }
issues-bulk-ignore-all = Ignorer les { $count }
issues-bulk-delete-all = Supprimer les { $count }
issues-bulk-resolve-confirm = { $count ->
    [one] Résoudre le { $count } problème correspondant ?
   *[other] Résoudre les { $count } problèmes correspondants ?
}
issues-bulk-ignore-confirm = { $count ->
    [one] Ignorer le { $count } problème correspondant ?
   *[other] Ignorer les { $count } problèmes correspondants ?
}
issues-bulk-delete-all-confirm = { $count ->
    [one] Supprimer définitivement le { $count } problème correspondant ?
   *[other] Supprimer définitivement les { $count } problèmes correspondants ?
}
issues-bulk-resolve = Résoudre
issues-bulk-ignore = Ignorer
issues-bulk-delete = Supprimer
issues-bulk-delete-selected-confirm = Supprimer définitivement les problèmes sélectionnés ?

# --- Nombre (pagination) ---
issues-count = { $count ->
    [one] { $count } problème
   *[other] { $count } problèmes
}

# --- Détail d'un problème ---
issue-detail-title-fallback = Problème
issue-detail-resolve = ✓ Résoudre
issue-detail-reopen = Rouvrir
issue-detail-unignore = Ne plus ignorer
issue-detail-tab-details = Détails
issue-detail-tab-events = Tous les événements
issue-detail-exception-stacktrace = Exception &amp; Stacktrace
issue-detail-handled = gérée
issue-detail-unhandled = non gérée
issue-detail-in = dans
issue-detail-var-name = Variable
issue-detail-no-source = Aucun contexte de code source disponible
issue-detail-minified-hint = Ces frames semblent minifiées et aucune source map n'a été appliquée.
issue-detail-minified-hint-link = Téléverser des source maps
issue-detail-breadcrumbs = Fil d'Ariane
issue-detail-th-time = Heure
issue-detail-th-category = Catégorie
issue-detail-th-message = Message
issue-detail-crumb-data = données
issue-detail-tags = Tags
issue-detail-contexts = Contextes
issue-detail-additional-data = Données supplémentaires
issue-detail-view-replay = Voir le replay
issue-detail-view-trace = Voir la trace
issue-detail-request = Requête
issue-detail-headers = En-têtes
issue-detail-th-header = En-tête
issue-detail-query-string = Chaîne de requête
issue-detail-body = Corps
issue-detail-environment = Environnement
issue-detail-user-reports = Rapports utilisateurs
issue-detail-anonymous = Anonyme
issue-detail-attachments = Pièces jointes
issue-detail-att-filename = Nom de fichier
issue-detail-att-type = Type
issue-detail-att-size = Taille
issue-detail-download = Télécharger
issue-detail-raw-json = JSON brut
issue-detail-no-events = Aucun événement trouvé pour ce problème.
issue-detail-ev-id = ID de l'événement
issue-detail-ev-timestamp = Horodatage
issue-detail-ev-platform = Plateforme
issue-detail-events-count = { $count ->
    [one] { $count } événement
   *[other] { $count } événements
}
issue-detail-props-heading = Propriétés du problème
issue-detail-fingerprint = Empreinte
issue-detail-tag-facets = Facettes de tags
issue-detail-discard-undo-title = Reprendre l'acceptation des futurs événements avec cette empreinte
issue-detail-discard-undo = Annuler le rejet
issue-detail-discard-confirm = Rejeter tous les futurs événements avec cette empreinte ?
issue-detail-discard-title = Rejeter silencieusement les futurs événements correspondant à cette empreinte
issue-detail-discard = Rejeter les futurs événements
