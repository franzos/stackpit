# Page des alertes et récapitulatifs (templates/alerts.html). Réutilise
# nav-settings et nav-alerts-digests pour les éléments de chrome. Les espaces
# de séparation vivent dans le template, donc les valeurs ne portent pas
# d'espace au début/à la fin. alerts-page-title conserve l'entité brute &amp;
# et est rendu avec |safe.
alerts-page-title = Alertes &amp; récapitulatifs — Stackpit
alerts-notify-help-pre = Les notifications sont envoyées via les intégrations de la page
alerts-notify-help-post = .

# --- Règles de seuil ---
alerts-threshold-heading = Règles de seuil
alerts-threshold-desc = Se déclenche lorsqu'un problème reçoit plus de N événements dans une fenêtre de temps.
alerts-rules-empty = Aucune règle d'alerte pour le moment.
alerts-col-scope = Portée
alerts-col-issue = Problème
alerts-col-threshold = Seuil
alerts-col-window = Fenêtre
alerts-col-cooldown = Temporisation
alerts-scope-global = Global
alerts-fingerprint-any = Tous
alerts-rule-delete-confirm = Supprimer cette règle d'alerte ?
alerts-delete-label = Supprimer
alerts-add-rule = + Ajouter une règle d'alerte
alerts-all-projects = Tous les projets
alerts-project-fallback = Projet { $id }
alerts-fingerprint-label = Empreinte du problème
alerts-fingerprint-hint = (vide = tous)
alerts-fingerprint-placeholder = tout problème
alerts-fingerprint-help = Une empreinte identifie un problème (événements groupés). Visible dans l'URL de chaque page de problème. Laissez vide pour couvrir tous les problèmes de la portée.
alerts-unit-s = (s)
alerts-create-rule = Créer la règle

# --- Planifications de récapitulatifs ---
alerts-digest-heading = Planifications de récapitulatifs
alerts-digest-desc = Résumés d'activité périodiques : des points quotidiens ou hebdomadaires plutôt que du bruit à chaque événement.
alerts-digests-empty = Aucune planification de récapitulatif pour le moment.
alerts-col-interval = Intervalle
alerts-col-last-sent = Dernier envoi
alerts-col-enabled = Activé
alerts-never = Jamais
alerts-yes = Oui
alerts-no = Non
alerts-digest-delete-confirm = Supprimer cette planification de récapitulatif ?
alerts-add-digest = + Ajouter une planification de récapitulatif
alerts-interval-daily = Quotidien (24 h)
alerts-interval-weekly = Hebdomadaire (7 j)
alerts-interval-hourly = Toutes les heures
alerts-create-schedule = Créer la planification
