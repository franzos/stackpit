# Interface des intégrations : la liste (templates/integrations.html) et les
# trois formulaires d'ajout (webhook, Slack, e-mail). Réutilise
# nav-settings/nav-integrations. integrations-empty contient du markup <strong>
# inline et le glyphe de flèche, rendu avec |safe.
integrations-page-title = Intégrations — Stackpit
integrations-subtitle = Sorties webhook, Slack et e-mail. Le routage par projet se configure dans les paramètres de chaque projet.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + E-mail
integrations-license-required-badge = Licence requise
integrations-empty = Aucune intégration pour le moment. Ajoutez-en une ci-dessus pour commencer à recevoir des notifications. Après l'ajout, activez-la par projet dans <strong>Paramètres du projet → Intégrations</strong>.
integrations-col-name = Nom
integrations-col-type = Type
integrations-col-endpoint = Point de terminaison
integrations-col-created = Créé
integrations-delete-confirm = Supprimer cette intégration ? Elle sera retirée de tous les projets.
integrations-test = Tester
integrations-delete = Supprimer
flash-test-failed = Échec du test : { $error }

# Étiquettes/boutons de formulaire partagés par les trois formulaires d'ajout.
integrations-cancel = Annuler
integrations-optional = (facultatif)
integrations-required = (requis)
integrations-create = Créer l'intégration

# --- Ajouter un webhook ---
integrations-webhook-title = Ajouter un webhook — Stackpit
integrations-webhook-breadcrumb = Ajouter un webhook
integrations-webhook-heading = Ajouter une intégration webhook
integrations-webhook-name-placeholder = p. ex. Alertes de production
integrations-webhook-url-label = URL du webhook
integrations-webhook-secret-label = Secret HMAC
integrations-webhook-secret-placeholder = Secret de signature facultatif

# --- Ajouter Slack ---
integrations-slack-title = Ajouter Slack — Stackpit
integrations-slack-breadcrumb = Ajouter Slack
integrations-slack-heading = Ajouter une intégration Slack
integrations-slack-name-placeholder = p. ex. canal #alerts
integrations-slack-url-label = URL du webhook Slack

# --- Ajouter un e-mail ---
integrations-email-title = Ajouter un e-mail — Stackpit
integrations-email-breadcrumb = Ajouter un e-mail
integrations-email-heading = Ajouter une intégration e-mail
integrations-email-name-placeholder = p. ex. Alertes e-mail de l'équipe
integrations-email-lock-pre = Le fournisseur et l'expéditeur proviennent de la
integrations-email-lock-post = configuration du serveur ; cette intégration ne choisit que le destinataire.
integrations-email-provider-label = Fournisseur
integrations-email-token-label = Jeton d'API
integrations-email-token-placeholder-default = Laisser vide pour utiliser la valeur par défaut
integrations-email-token-placeholder = Jeton d'API du fournisseur
integrations-email-from-label = Adresse d'expéditeur
integrations-email-fromname-label = Nom d'expéditeur
integrations-email-smtp-hint = SMTP utilise la connexion [email] du serveur ; aucun jeton par intégration n'est nécessaire.

# Gestionnaire de tickets
integrations-add-tracker = + Gestionnaire de tickets
integrations-tracker-title = Ajouter un gestionnaire de tickets — Stackpit
integrations-tracker-breadcrumb = Ajouter un gestionnaire de tickets
integrations-tracker-heading = Ajouter une intégration de gestionnaire de tickets
integrations-tracker-kind-label = Gestionnaire
integrations-tracker-name-placeholder = ex. GitHub Issues
integrations-tracker-url-label = URL de base
integrations-tracker-token-label = Jeton d'API
integrations-tracker-token-placeholder = Jeton d'accès personnel
integrations-tracker-target-help = Le dépôt visé provient des paramètres de dépôt de chaque projet et ne se configure donc pas ici. Ajoutez le dépôt dans les paramètres du projet.
integrations-global-label = Diffuser à tous les projets
integrations-global-help = Les alertes vont à tous les projets de cette organisation, sauf ceux que vous excluez sur la page de cette intégration. Les filtres de niveau et d'environnement par projet s'appliquent en plus.
integrations-global-badge = organisation
integrations-global-save = Enregistrer la diffusion
integrations-global-on = Diffuser à toute l'organisation
integrations-global-off = Arrêter la diffusion à toute l'organisation

# Détail de l'intégration : diffusion par projet
integrations-detail-title = Intégration — Stackpit
integrations-back = Retour aux intégrations
integrations-projects-heading = Diffusion par projet
integrations-projects-hint-global = Cette intégration diffuse à tous les projets ci-dessous, sauf ceux que vous excluez. L'exclusion est la seule sortie possible ; il n'y a pas de liste d'inclusion.
integrations-projects-hint-per-project = Cette intégration ne diffuse que là où un projet l'a activée. Marquez-la « organisation » pour diffuser partout.
integrations-projects-hint-tracker = Les gestionnaires de tickets sont associés aux dépôts d'un projet par forge et par hôte. Exclure un projet retire ce gestionnaire de ses options de création.
integrations-projects-empty = Cette organisation n'a encore aucun projet.
integrations-col-project = Projet
integrations-col-state = État
integrations-project-archived = archivé
integrations-state-default = Diffuse
integrations-state-customised = Personnalisé
integrations-state-excluded = Exclu
integrations-state-no-repo = Aucun dépôt correspondant
integrations-state-not-routed = Non activé
integrations-exclude = Exclure
integrations-include = Inclure
integrations-email-to-label = Destinataire par défaut
integrations-email-to-help = Utilisé là où un projet n'a pas défini sa propre adresse. Obligatoire pour une intégration à l'échelle de l'organisation.
