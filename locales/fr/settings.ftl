# Interface des paramètres : la page des valeurs par défaut du navigateur
# (templates/browser_defaults.html, clés defaults-*) et la page autonome de
# provisionnement des organisations (templates/provision.html, clés provision-*).
# Réutilise nav-settings. Les valeurs de niveau (fatal/error/...) restent
# littérales dans le template, comme sur les interfaces problèmes/événements.

# --- Valeurs par défaut du navigateur ---
defaults-page-title = Valeurs par défaut du navigateur — Stackpit
defaults-subtitle = Définissez les valeurs de filtre par défaut pour les pages de liste. Stockées dans un cookie du navigateur.
defaults-none = Aucune valeur par défaut
defaults-status-label = Statut par défaut (problèmes)
defaults-status-unresolved = Non résolu
defaults-status-resolved = Résolu
defaults-status-ignored = Ignoré
defaults-level-label = Niveau par défaut
defaults-period-label = Plage temporelle par défaut
defaults-save = Enregistrer les valeurs par défaut
defaults-clear-confirm = Effacer toutes les valeurs par défaut du navigateur ?
defaults-clear = Effacer toutes les valeurs par défaut
flash-defaults-saved = Valeurs par défaut enregistrées
flash-defaults-cleared = Valeurs par défaut effacées

# --- Langue préférée ---
settings-language-heading = Langue préférée
settings-language-subtitle = Choisissez la langue de l'interface Stackpit. Pour les comptes connectés, ce choix est conservé sur tous les appareils.
settings-language-label = Langue
settings-language-save = Enregistrer la langue

settings-aria-sections = Sections des paramètres

# --- Page de provisionnement (page autonome) ---
provision-page-title = Configurer les organisations — Stackpit
provision-heading = Configurer les organisations
provision-subtitle-1 = Les organisations suivantes sont disponibles via votre fournisseur d'identité.
provision-subtitle-2 = Sélectionnez celles que vous souhaitez créer dans Stackpit.
provision-create = Créer la sélection
provision-skip = Ignorer

# File de diffusion
queue-page-title = File de diffusion — Stackpit
queue-subtitle = Notifications qui n'ont pas pu être livrées. Elles sont réessayées automatiquement pendant 24 heures, puis vous attendent ici.
queue-count-pending = { $count } en attente
queue-count-failed = { $count } en échec
queue-empty = Rien en attente. Toutes les notifications ont été livrées.
queue-col-integration = Intégration
queue-col-project = Projet
queue-col-state = État
queue-col-attempts = Tentatives
queue-col-queued = Mise en file
queue-col-error = Dernière erreur
queue-state-pending = Réessai en cours
queue-state-failed = Abandonné
queue-replay = Renvoyer
queue-cancel = Abandonner
queue-cancel-confirm = Abandonner cette notification sans la livrer ?
