# Interface des projets : liste, nouveau, paramètres (général/clés/source maps/
# filtres), intégrations et la confirmation de création. Les valeurs rendues
# avec |safe portent du markup HTML inline ; les balises restent identiques,
# seul le texte est traduit.

# --- Liste des projets ---
projects-list-title = Projets — Stackpit
projects-list-heading = Projets
projects-list-subtitle = Surveillez la santé de toute votre architecture.
projects-list-all-events = Tous les événements
projects-list-all-releases = Toutes les releases
projects-list-new = + Nouveau projet
projects-list-search-placeholder = Rechercher des projets par nom, plateforme ou propriétaire…
projects-list-search-label = Rechercher des projets
projects-list-filter = Filtrer
projects-list-empty = Aucun projet trouvé. Les événements apparaîtront ici une fois ingérés.
projects-period-label = Plage temporelle
projects-period-all = Tout l'historique
projects-period-1h = Dernière heure
projects-period-24h = Dernières 24 heures
projects-period-7d = 7 derniers jours
projects-period-14d = 14 derniers jours
projects-period-30d = 30 derniers jours
projects-period-90d = 90 derniers jours
projects-period-365d = 365 derniers jours
projects-col-project = Projet
projects-col-platforms = Plateformes
projects-col-issues = Problèmes
projects-col-events = Événements
projects-col-breakdown = Répartition
projects-col-release = Release
projects-col-first-seen = Première apparition
projects-col-last-seen = Dernière apparition
projects-breakdown-errors = Erreurs :
projects-breakdown-transactions = Transactions :
projects-breakdown-sessions = Sessions :
projects-breakdown-other = Autres :
projects-legend-errors = Erreurs
projects-legend-transactions = Transactions
projects-legend-sessions = Sessions
projects-legend-other = Autres

# --- Partagé entre les formulaires de projet ---
projects-optional = (facultatif)
projects-cancel = Annuler
projects-remove = Retirer
projects-delete = Supprimer
projects-name-placeholder = Mon projet

# --- Nouveau projet ---
projects-new-title = Nouveau projet — Stackpit
projects-new-heading = Nouveau projet
projects-new-name-label = Nom du projet
projects-new-platform-label = Plateforme
projects-new-platform-select = Sélectionner une plateforme…
projects-new-platform-other = Autre
projects-new-platform-native = Native (C/C++)
projects-new-submit = Créer le projet

# --- Onglets de paramètres (partagés par les pages de paramètres) ---
projects-tab-general = Général
projects-tab-sdk = Configuration du SDK
projects-tab-sourcemaps = Source maps
projects-tab-filters = Filtres
projects-tab-integrations = Intégrations

# --- Paramètres : général ---
projects-settings-heading = Paramètres
projects-settings-archived = (archivé)
projects-settings-name-heading = Nom du projet
projects-settings-display-name = Nom d'affichage
projects-settings-save-name = Enregistrer le nom
projects-settings-info-heading = Informations du projet
projects-settings-status = Statut
projects-settings-source = Source
projects-repos-heading = Dépôts de code source
projects-repos-help = Reliez les frames de la pile au code source sur votre forge. Enregistrez une release avec un SHA de commit via <code class="text-mono">sentry-cli</code> pour activer les liens.
projects-repos-empty = Aucun dépôt configuré.
projects-repos-url-label = URL du dépôt
projects-repos-col-forge = Forge
projects-repos-template = Modèle d'URL
projects-repos-auto = auto
projects-repos-remove-confirm = Retirer ce dépôt ?
projects-repos-add = Ajouter un dépôt
projects-repos-add-help = Ajoute des liens de source cliquables (p. ex. « Voir sur GitHub ») à côté des frames de la pile. Nécessite une release avec un SHA de commit : le type de forge est détecté automatiquement. Pris en charge : GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. Pour les autres forges, fournissez un modèle d'URL.
projects-danger-heading = Zone de danger
projects-archive-desc = Archiver ce projet. Les projets archivés rejettent les nouveaux événements.
projects-archive-confirm = Archiver ce projet ? Les nouveaux événements seront rejetés.
projects-archive-submit = Archiver le projet
projects-unarchive-desc = Désarchiver ce projet pour recommencer à accepter des événements.
projects-unarchive-submit = Désarchiver le projet
projects-delete-desc = Supprimer définitivement ce projet et toutes ses données. Cette action est irréversible.
projects-delete-confirm = Supprimer ce projet et TOUTES ses données ? Cette action est irréversible.
projects-delete-submit = Supprimer le projet
projects-move-heading = Déplacer vers une organisation
projects-move-desc = Déplacez ce projet vers une autre organisation dont vous êtes propriétaire. Ses données et ses DSN restent valides, mais les intégrations de notification sont dissociées et doivent être rajoutées dans la nouvelle organisation.
projects-move-target-label = Organisation de destination
projects-move-confirm-pre = Saisissez
projects-move-confirm-post = pour confirmer.
projects-move-confirm-placeholder = Nom du projet
projects-move-confirm-dialog = Déplacer ce projet vers l'organisation sélectionnée ?
projects-move-submit = Déplacer le projet
projects-move-err-invalid-target = Organisation de destination invalide.
projects-move-err-name-mismatch = Le nom du projet ne correspond pas.
projects-move-err-denied = Vous n'êtes pas propriétaire de l'organisation de destination.
projects-move-err-conflict = Impossible de déplacer le projet ; il a peut-être changé. Veuillez réessayer.

# --- Paramètres : configuration du SDK / clés ---
projects-keys-title = Configuration du SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = Aucune clé enregistrée. Créez une clé ci-dessous pour obtenir une DSN.
projects-keys-list-heading = Clés du projet
projects-keys-empty = Aucune clé enregistrée pour ce projet.
projects-keys-col-public = Clé publique
projects-keys-col-label = Libellé
projects-keys-col-status = Statut
projects-keys-col-created = Créé
projects-keys-delete-confirm = Supprimer cette clé ? Les SDK qui l'utilisent cesseront de fonctionner.
projects-keys-create-heading = Créer une clé
projects-keys-label-label = Libellé
projects-keys-label-placeholder = p. ex. production, staging
projects-keys-create-submit = Créer la clé

# --- Paramètres : source maps ---
projects-sourcemaps-title = Source maps
projects-sourcemaps-apikey-heading = Clé d'API
projects-sourcemaps-apikey-desc = L'envoi de source maps nécessite une clé d'API. Spécifique à ce projet et utilisable uniquement pour les opérations sur les source maps.
projects-sourcemaps-key-generated = Clé générée :
projects-sourcemaps-key-warning = Copiez cette clé maintenant : elle ne sera plus affichée.
projects-sourcemaps-col-key = Clé
projects-sourcemaps-regen-confirm = Régénérer la clé ? La clé actuelle cessera de fonctionner.
projects-sourcemaps-regen = Régénérer
projects-sourcemaps-empty = Aucune clé d'API de source maps pour ce projet.
projects-sourcemaps-generate = Générer une clé
projects-sourcemaps-setup-heading = Configuration
projects-sourcemaps-setup-desc = Utilisez <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> pour envoyer les source maps. Définissez ces variables d'environnement :
projects-sourcemaps-then-upload = Puis envoyez :

# --- Paramètres : filtres ---
projects-filters-inbound-heading = Filtres entrants
projects-filters-inbound-desc = Filtres intégrés qui rejettent les événements correspondant aux motifs de bruit courants.
projects-filters-browser-ext = Extensions de navigateur : rejeter les événements provenant des extensions Chrome/Firefox/Safari
projects-filters-localhost = Localhost : rejeter les événements provenant de localhost, 127.0.0.1, des IP privées
projects-filters-inbound-submit = Enregistrer les filtres entrants
projects-filters-message-heading = Filtres de message
projects-filters-message-help = Motifs glob comparés aux titres des événements. Utilisez <code class="text-mono">*</code> pour une séquence quelconque, <code class="text-mono">?</code> pour un seul caractère.
projects-filters-col-pattern = Motif
projects-filters-message-empty = Aucun filtre de message configuré.
projects-filters-add-pattern = Ajouter un motif
projects-filters-message-submit = Ajouter un filtre de message
projects-filters-ratelimit-heading = Limite de débit
projects-filters-ratelimit-desc = Nombre maximal d'événements par minute pour ce projet. 0 = illimité.
projects-filters-ratelimit-label = Événements par minute
projects-filters-ratelimit-submit = Enregistrer la limite de débit
projects-filters-env-heading = Environnements exclus
projects-filters-env-desc = Les événements de ces environnements seront rejetés silencieusement.
projects-filters-col-environment = Environnement
projects-filters-env-empty = Aucun environnement exclu.
projects-filters-env-add-label = Ajouter un environnement exclu
projects-filters-env-submit = Exclure l'environnement
projects-filters-release-heading = Filtres de release
projects-filters-release-desc = Motifs glob comparés aux versions de release. Les événements correspondants sont rejetés.
projects-filters-release-empty = Aucun filtre de release.
projects-filters-release-submit = Ajouter un filtre de release
projects-filters-ua-heading = Filtres de user-agent
projects-filters-ua-desc = Motifs glob comparés aux en-têtes User-Agent. Les motifs intégrés pour kube-probe et les vérificateurs de santé sont toujours actifs.
projects-filters-ua-empty = Aucun filtre de user-agent personnalisé.
projects-filters-ua-submit = Ajouter un filtre de user-agent
projects-filters-rules-heading = Règles personnalisées
projects-filters-rules-desc = Règles avancées qui comparent les champs des événements. Les règles de priorité plus élevée sont évaluées en premier.
projects-filters-col-field = Champ
projects-filters-col-operator = Opérateur
projects-filters-col-value = Valeur
projects-filters-col-action = Action
projects-filters-col-priority = Priorité
projects-filters-rules-empty = Aucune règle personnalisée.
projects-filters-sample-rate-label = Taux d'échantillonnage
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Ajouter une règle
projects-filters-op = { $op ->
    [not_equals] différent de
    [contains] contient
    [not_contains] ne contient pas
    [starts_with] commence par
    [in] dans la liste
    [not_in] hors de la liste
   *[equals] égal à
}
projects-filters-action = { $action ->
    [sample] échantillonner
   *[drop] rejeter
}
projects-filters-ip-heading = Liste de blocage d'IP
projects-filters-ip-desc = Blocs CIDR ou IP individuelles. Les événements des IP bloquées sont rejetés silencieusement.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = Aucun bloc d'IP configuré.
projects-filters-ip-add-label = Ajouter un CIDR
projects-filters-ip-submit = Bloquer la plage d'IP
projects-filters-discard-heading = Statistiques de rejet
projects-filters-discard-window = (7 derniers jours)
projects-filters-col-date = Date
projects-filters-col-reason = Raison
projects-filters-col-count = Nombre

# Étiquettes d'entité de filtre, interpolées dans flash-not-found-filter à la suppression.
projects-filter-label-message = filtre de message
projects-filter-label-environment = filtre d'environnement
projects-filter-label-release = filtre de release
projects-filter-label-user-agent = filtre de user-agent
projects-filter-label-rule = règle de filtre

# --- Paramètres : intégrations ---
projects-integrations-active-heading = Intégrations actives
projects-integrations-active-empty = Aucune intégration activée. Ajoutez d'abord une intégration globale sur la page <a class="text-primary" href="/web/settings/integrations/">Intégrations</a>, puis activez-la ici. Vous pouvez limiter chacune par niveau minimum et par environnement pour que le bruit de dev reste hors des canaux de prod.
projects-integrations-deactivate-confirm = Désactiver cette intégration pour le projet ?
projects-integrations-deactivate = Désactiver
projects-integrations-notify-new-issues = Nouveaux problèmes
projects-integrations-notify-regressions = Régressions
projects-integrations-notify-threshold = Alertes de seuil
projects-integrations-notify-digests = Récapitulatifs
projects-integrations-min-level = Niveau minimum
projects-integrations-level-any = Tous
projects-integrations-env-filter = Filtre d'environnement
projects-integrations-env-placeholder = p. ex. production
projects-integrations-to-address = Adresse du destinataire
projects-integrations-to-address-note = (intégrations e-mail uniquement)
projects-integrations-activate-heading = Activer l'intégration
projects-integrations-integration-label = Intégration
projects-integrations-activate-submit = Activer
projects-integrations-available-empty = Aucune intégration disponible. <a class="text-primary" href="/web/settings/integrations/">Créez-en une d'abord</a>.

# --- Projet créé ---
projects-created-word = créé
projects-created-breadcrumb = Créé
projects-created-heading = Projet créé
projects-created-subtitle = Utilisez la DSN ci-dessous pour configurer votre SDK.
projects-created-settings-btn = Paramètres du projet
projects-created-back = Retour aux projets
projects-created-details-heading = Détails du projet
projects-created-col-id = ID du projet
projects-created-sdk-desc-before = Installez le SDK Sentry pour
projects-created-sdk-desc-after = et initialisez-le avec la DSN ci-dessus.
projects-created-docs-javascript = Doc Sentry JavaScript →
projects-created-docs-python = Doc Sentry Python →
projects-created-docs-rust = Doc Sentry Rust →
projects-created-docs-go = Doc Sentry Go →
projects-created-docs-node = Doc Sentry Node.js →
projects-created-docs-java = Doc Sentry Java →
projects-created-docs-ruby = Doc Sentry Ruby →
projects-created-docs-php = Doc Sentry PHP →
projects-created-docs-elixir = Doc Sentry Elixir →
projects-created-docs-dotnet = Doc Sentry .NET →
projects-created-docs-apple = Doc Sentry Apple →
projects-created-docs-kotlin = Doc Sentry Kotlin →
projects-created-docs-native = Doc Sentry Native →
projects-created-docs-generic = Doc Sentry plateforme →
