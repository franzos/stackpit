# Équivalents français de locales/en/errors.ftl. Le nom de marque « Stackpit »
# reste littéral dans les templates, comme dans base.html/login.html.
error-page-title = Erreur - Stackpit
error-heading = Erreur
error-back-projects = Retour aux projets

# Page de confirmation d'invitation créée (locale par défaut/anglais uniquement).
invite-created-page-title = Invitation créée - Stackpit
invite-created-heading = Invitation créée
invite-created-share = Partagez ce lien. Il est valable { $ttl } et à usage unique.
invite-created-back-members = Retour aux membres

# --- Messages flash, de succès et de validation (dépendants de la locale) ---
# Émis par les gestionnaires web sous forme de bannière ponctuelle. Le préfixe
# dynamique « Erreur : » est ajouté en Rust via common-error-prefix.

# Diagnostics « introuvable ». Le préfixe « Erreur : » est ajouté en Rust ; la
# valeur ne porte que l'expression de l'entité et l'id.
flash-not-found-project = projet introuvable : { $id }
flash-not-found-key = clé d'API introuvable : { $id }
flash-not-found-integration = intégration introuvable : { $id }
flash-not-found-alert-rule = règle d'alerte introuvable : { $id }
flash-not-found-digest-schedule = planification de récapitulatif introuvable : { $id }
flash-not-found-repo = dépôt introuvable : { $id }
flash-not-found-project-integration = intégration de projet introuvable : { $id }
flash-not-found-filter = { $label } introuvable

# Validation des règles de filtre
flash-unrecognized-field = Champ non reconnu : { $value }
flash-unrecognized-operator = Opérateur non reconnu : { $value }
flash-unrecognized-action = Action non reconnue : { $value }

# Paramètres du projet
flash-project-name-updated = Nom du projet mis à jour
flash-project-name-too-long = Le nom du projet dépasse la longueur maximale de { $max } caractères
flash-repo-url-required = L'URL du dépôt est requise
flash-repo-url-too-long = L'URL du dépôt dépasse la longueur maximale de 2048 caractères
flash-repo-added = Dépôt ajouté
flash-repo-removed = Dépôt retiré
flash-project-archived = Projet archivé
flash-project-unarchived = Projet désarchivé
flash-key-created = Clé créée
flash-key-deleted = Clé supprimée

# Alertes et récapitulatifs
flash-project-not-found-or-denied = Erreur : projet introuvable ou accès refusé
flash-alert-rule-created = Règle d'alerte créée
flash-alert-rule-deleted = Règle d'alerte supprimée
flash-digest-schedule-created = Planification de récapitulatif créée
flash-digest-schedule-deleted = Planification de récapitulatif supprimée

# Intégrations de projet
flash-integration-not-found = Intégration introuvable
flash-integration-activated = Intégration activée
flash-integration-updated = Intégration mise à jour
flash-integration-deactivated = Intégration désactivée

# Intégrations d'organisation
flash-name-required = Le nom est requis
flash-invalid-integration-kind = Type d'intégration invalide
flash-invalid-email-provider = Fournisseur d'e-mail invalide
flash-api-token-required = Le jeton d'API est requis.
flash-from-address-required = L'adresse d'expéditeur est requise.
flash-smtp-not-configured = SMTP n'est pas configuré. Définissez [email.smtp] host dans la configuration du serveur.
flash-invalid-to-address = Le destinataire doit être une adresse e-mail valide.
flash-test-digest-sent = Digest de test mis en file pour { $count } projet(s) vers leurs intégrations avec digests activés.
flash-test-digest-sample = Aucune activité récente : un digest d'exemple étiqueté a été mis en file.
flash-test-digest-no-target = Aucune intégration n'a activé les digests pour le projet de cette planification.
flash-url-required = L'URL est requise
flash-secret-not-configured = Impossible d'enregistrer le secret : le chiffrement n'est pas configuré. Définissez STACKPIT_MASTER_KEY pour activer le stockage des secrets.
flash-integration-created = Intégration créée
flash-integration-name-exists = Une intégration portant ce nom existe déjà.
flash-integration-deleted = Intégration supprimée
flash-integration-no-url = Aucune URL n'est configurée pour l'intégration
flash-test-notification-sent = Notification de test envoyée

# Filtres entrants
flash-inbound-filters-updated = Filtres entrants mis à jour
flash-pattern-required = Le motif est requis
flash-message-filter-added = Filtre de message ajouté
flash-message-filter-removed = Filtre de message retiré
flash-rate-limit-updated = Limite de débit mise à jour
flash-environment-required = L'environnement est requis
flash-environment-excluded = Environnement exclu
flash-environment-filter-removed = Filtre d'environnement retiré
flash-release-filter-added = Filtre de release ajouté
flash-release-filter-removed = Filtre de release retiré
flash-ua-filter-added = Filtre de user-agent ajouté
flash-ua-filter-removed = Filtre de user-agent retiré
flash-rule-added = Règle ajoutée
flash-rule-removed = Règle retirée
flash-cidr-required = Le CIDR est requis
flash-invalid-cidr = Format CIDR invalide
flash-ip-block-added = Blocage d'IP ajouté
flash-ip-block-removed = Blocage d'IP retiré

# Nouveau projet
flash-project-name-required = Le nom du projet est requis
