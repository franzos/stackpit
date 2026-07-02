# Interface des organisations : la liste (templates/orgs.html), la page des
# membres/invitations (templates/org_members.html) et la page autonome
# d'acceptation d'invitation (templates/invite_accept.html, clés invite-*).
# Réutilise nav-organizations et common-action-save. Les espaces de séparation
# vivent dans le template. La phrase d'avertissement de suppression et
# l'étiquette de confirmation sont découpées à leurs interpolations {{ var }}.
orgs-page-title = Organisations - Stackpit
orgs-subtitle = Les organisations dont vous faites partie. Basculez entre elles ou créez-en une nouvelle.
orgs-empty = Vous n'êtes membre d'aucune organisation pour le moment.
orgs-col-organization = Organisation
orgs-col-kind = Type
orgs-members-btn = Membres
orgs-active = Active
orgs-switch = Basculer
orgs-create-heading = Créer une organisation
orgs-create-desc = Vous en devenez le propriétaire. Le slug est dérivé du nom s'il est laissé vide.
orgs-name = Nom
orgs-slug = Slug
orgs-optional = (facultatif)
orgs-create-submit = Créer l'organisation

# --- Page des membres ---
orgs-members-title-suffix = Membres - Stackpit
orgs-members-word = Membres
orgs-organization-word = organisation
orgs-slug-heading = Slug
orgs-slug-desc = Identifie cette organisation dans les URL. Doit être unique.
orgs-email = E-mail
orgs-role = Rôle
orgs-role-member = membre
orgs-role-owner = propriétaire
orgs-member-fallback = utilisateur #{ $id }
orgs-joined = Rejoint le
orgs-promote = Promouvoir
orgs-demote = Rétrograder
orgs-remove = Retirer
orgs-invites-heading = Invitations
orgs-created = Créé
orgs-expires = Expire
orgs-status = Statut
orgs-revoke = Révoquer
orgs-create-invite-heading = Créer une invitation
orgs-create-invite-desc = Génère un lien d'invitation à usage unique.
orgs-expiry-label = Expiration (secondes)
orgs-expiry-hint = (facultatif, 7 jours par défaut)
orgs-create-invite-submit = Créer l'invitation
orgs-forseti-note = L'appartenance à cette organisation est gérée en externe.
orgs-personal-note = Il s'agit d'une organisation personnelle. L'appartenance n'est pas configurable.
orgs-danger-heading = Zone de danger
orgs-delete-danger-pre = La suppression retire
orgs-delete-danger-projects = projet(s),
orgs-delete-danger-members = membre(s)
orgs-delete-danger-rest = ainsi que tous les événements, problèmes, clés, alertes et intégrations. Cette action est irréversible.
orgs-confirm-type-pre = Saisissez
orgs-confirm-type-post = pour confirmer
orgs-delete-confirm = Supprimer cette organisation et TOUTES ses données. Cette action est irréversible.
orgs-delete-submit = Supprimer l'organisation

# --- Acceptation d'invitation (page autonome) ---
invite-page-title = Invitation à une organisation - Stackpit
invite-heading = Invitation à une organisation
invite-back-projects = Retour aux projets
invite-intro-pre = Vous avez été invité à rejoindre
invite-intro-as = en tant que
invite-intro-post = .
invite-accept-btn = Accepter l'invitation
invite-decline = Refuser
invite-error-accepted = Cette invitation a déjà été acceptée.
invite-error-expired = Cette invitation a expiré.

# Messages de validation/erreur rendus sur la page html_error, localisés aux
# points d'appel qui portent une locale de requête. Les erreurs 5xx internes
# restent en anglais.
orgs-err-name-required = Le nom de l'organisation est requis.
orgs-err-slug-taken = Ce slug est déjà pris.
orgs-err-invite-not-found = Invitation introuvable ou invalide.
orgs-err-org-not-found = Organisation introuvable.
orgs-err-last-owner-remove = Le dernier propriétaire ne peut pas être retiré.
orgs-err-last-owner-demote = Le dernier propriétaire ne peut pas être rétrogradé.
orgs-err-confirm-slug = Saisissez le slug de l'organisation pour confirmer la suppression.
orgs-err-not-deletable = Cette organisation ne peut pas être supprimée.
orgs-err-limit-reached = { $count ->
    [one] Vous avez atteint la limite de { $count } organisation.
   *[other] Vous avez atteint la limite de { $count } organisations.
}
