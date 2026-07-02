# Page de connexion autonome (templates/login.html) ainsi que les textes de
# bannière OAuth/déconnexion générés dans src/html/login.rs. login-token-help
# contient du markup <code> inline et est rendu avec |safe.
login-page-title = Connexion — Stackpit
login-welcome = Bon retour
login-subtitle = Connectez-vous pour gérer votre suivi des erreurs
login-sso = Se connecter avec SSO
login-or = ou
login-token-label = Jeton d'administration
login-token-placeholder = Saisissez votre jeton principal…
login-submit = Se connecter
login-token-help = Le jeton d'administration provient de <code class="text-mono">admin_token</code> dans <code class="text-mono">stackpit.toml</code>. Modifiez le fichier et redémarrez <code class="text-mono">stackpit serve</code> pour prendre en compte les changements.
login-docs = Documentation
login-selfhosting = Guide d'auto-hébergement

# Bannière d'erreur (dérivée des codes ?error= de la redirection OAuth) et bannière d'info de déconnexion.
login-error-state-mismatch = Votre session de connexion a été altérée ou a expiré. Veuillez réessayer.
login-error-session-expired = Votre session a expiré. Veuillez vous reconnecter.
login-error-missing-response = Votre fournisseur d'identité a renvoyé une réponse incomplète. Veuillez réessayer.
login-error-token-exchange = Nous n'avons pas pu finaliser la connexion avec votre fournisseur d'identité. Veuillez réessayer dans un instant.
login-error-provisioning = Votre compte n'a pas pu être créé. Contactez votre administrateur.
login-error-email-conflict = Un compte avec cette adresse e-mail existe déjà. Contactez votre administrateur.
login-error-session-unavailable = La connexion est temporairement indisponible. Veuillez réessayer dans un instant.
login-error-encryption = La connexion est mal configurée sur ce déploiement. Contactez votre administrateur.
login-error-generic = Échec de la connexion. Veuillez réessayer.
login-error-invalid-token = Jeton invalide
login-logout-local = Déconnecté de Stackpit. Votre session chez le fournisseur d'identité n'a pas été fermée -- déconnectez-vous-y séparément si nécessaire.
