# Interface des événements : la liste inter-projets et la page de détail d'un
# événement. event-detail-exception-stacktrace contient un &amp; inline et est
# rendu avec |safe. Les chaînes comptées utilisent les pluriels tv_count.

# --- Étiquettes partagées (liste + détail) ---
events-label-title = Titre
events-label-type = Type
events-label-level = Niveau
events-label-platform = Plateforme
events-label-environment = Environnement
events-label-time = Heure
events-label-value = Valeur

# --- Pagination (partagée) ---
events-pagination-label = Pagination
events-pagination-prev = « Précédent
events-pagination-next = Suivant »

# --- Suffixe de titre de page (titres à préfixe dynamique) ---
events-title-suffix = — Stackpit

# --- Liste des événements ---
events-list-title = Événements — Stackpit
events-heading = Événements
events-list-search-placeholder = Rechercher des événements…
events-list-search-label = Rechercher des événements
events-list-select = Sélectionner l'événement
events-list-filter-level = Filtrer par niveau
events-list-level-all = Tous les niveaux
events-list-filter-type = Filtrer par type
events-list-type-all = Tous les types
events-list-project-placeholder = ID du projet
events-list-filter-project = Filtrer par projet
events-list-filter-submit = Filtrer
events-list-empty = Aucun événement ne correspond aux filtres actuels.
events-untitled = (sans titre)
events-col-project = Projet

# --- Actions groupées ---
events-bulk-delete = Supprimer
events-bulk-delete-selected-confirm = Supprimer les événements sélectionnés ?
events-bulk-delete-all = Supprimer les { $count } correspondants
events-bulk-delete-all-confirm = { $count ->
    [one] Supprimer définitivement le { $count } événement correspondant ?
   *[other] Supprimer définitivement les { $count } événements correspondants ?
}

# --- Nombre (pagination) ---
events-count = { $count ->
    [one] { $count } événement
   *[other] { $count } événements
}

# --- Détail d'un événement ---
event-detail-event = Événement
event-detail-event-id-label = event_id:
event-detail-nav-label = Navigation entre événements
event-detail-nav-newer = « Plus récents
event-detail-nav-older = Plus anciens »
event-detail-nav-count = { $count ->
    [one] { $count } événement
   *[other] { $count } événements
}
event-detail-nav-in-issue = dans le problème
event-detail-user-feedback = Retour utilisateur
event-detail-anonymous = Anonyme
event-detail-related-event = Événement associé :
event-detail-exception-stacktrace = Exception &amp; Stacktrace
event-detail-handled = gérée
event-detail-unhandled = non gérée
event-detail-in = dans
event-detail-var-name = Variable
event-detail-no-source = Aucun contexte de code source disponible
event-detail-breadcrumbs = Fil d'Ariane
event-detail-th-category = Catégorie
event-detail-th-message = Message
event-detail-tags = Tags
event-detail-contexts = Contextes
event-detail-request = Requête
event-detail-headers = En-têtes
event-detail-th-header = En-tête
event-detail-query-string = Chaîne de requête
event-detail-body = Corps
event-detail-user-reports = Rapports utilisateurs
event-detail-attachments = Pièces jointes
event-detail-att-filename = Nom de fichier
event-detail-att-size = Taille
event-detail-download = Télécharger
event-detail-web-vitals = Web Vitals
event-detail-raw-json = JSON brut
event-detail-props-heading = Propriétés de l'événement
event-detail-prop-event-id = ID de l'événement
event-detail-prop-timestamp = Horodatage
event-detail-prop-transaction = Transaction
event-detail-prop-release = Release
event-detail-prop-server = Serveur
event-detail-prop-sdk = SDK
event-detail-prop-received = Reçu
event-detail-user-heading = Utilisateur
event-detail-user-id = ID
event-detail-user-email = E-mail
event-detail-user-username = Nom d'utilisateur
event-detail-user-ip = Adresse IP

# --- Rapports client (événements rejetés) ---
# Réutilise events-untitled et events-pagination-* (partagés, même fichier).
client-reports-title = Rapports client
client-reports-dropped-heading = Événements rejetés
client-reports-dropped-subtitle = Ce que les SDK ont écarté avant l'envoi, par catégorie et par raison.
client-reports-th-category = Catégorie
client-reports-th-reason = Raison
client-reports-th-reasons = Raisons
client-reports-th-dropped = Rejetés
client-reports-empty = Aucun rapport client trouvé pour ce projet.
client-reports-reports-heading = Rapports
client-reports-delete = Supprimer
client-reports-delete-selected-confirm = Supprimer les rapports sélectionnés ?
client-reports-th-event-id = ID de l'événement
client-reports-th-title = Titre
client-reports-th-timestamp = Horodatage
client-reports-th-platform = Plateforme
client-reports-th-release = Release
client-reports-select = Sélectionner le rapport
client-reports-delete-all = Supprimer les { $count }
client-reports-delete-all-confirm = { $count ->
    [one] Supprimer le { $count } rapport correspondant ?
   *[other] Supprimer les { $count } rapports correspondants ?
}
client-reports-count = { $count ->
    [one] { $count } rapport
   *[other] { $count } rapports
}

# --- Rapports utilisateurs (retour utilisateur) ---
# Réutilise events-untitled et events-pagination-* (partagés, même fichier).
user-reports-title = Rapports utilisateurs
user-reports-heading = Rapports utilisateurs
user-reports-empty = Aucun rapport utilisateur trouvé pour ce projet.
user-reports-delete = Supprimer
user-reports-delete-selected-confirm = Supprimer les rapports sélectionnés ?
user-reports-th-event-id = ID de l'événement
user-reports-th-title = Titre
user-reports-th-timestamp = Horodatage
user-reports-th-platform = Plateforme
user-reports-th-release = Release
user-reports-select = Sélectionner le rapport
user-reports-delete-all = Supprimer les { $count }
user-reports-delete-all-confirm = { $count ->
    [one] Supprimer le { $count } rapport correspondant ?
   *[other] Supprimer les { $count } rapports correspondants ?
}
user-reports-count = { $count ->
    [one] { $count } rapport
   *[other] { $count } rapports
}
