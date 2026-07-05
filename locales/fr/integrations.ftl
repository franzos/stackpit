# Interface des intégrations : la liste (templates/integrations.html) et les
# trois formulaires d'ajout (webhook, Slack, e-mail). Réutilise
# nav-settings/nav-integrations. integrations-empty contient du markup <strong>
# inline et le glyphe de flèche, rendu avec |safe.
integrations-page-title = Intégrations — Stackpit
integrations-subtitle = Sorties webhook, Slack et e-mail. Le routage par projet se configure dans les paramètres de chaque projet.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + E-mail
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
integrations-email-smtp-hint = SMTP utilise la connexion [email.smtp] du serveur ; aucun jeton par intégration n'est nécessaire.
