# Superficie organizzazioni: l'elenco org (templates/orgs.html), la pagina
# membri/inviti (templates/org_members.html) e la pagina autonoma di accettazione
# invito (templates/invite_accept.html, chiavi invite-*). Usa nav-organizations e
# common-action-save. Gli spazi separatori stanno nel template. La frase di
# pericolo e l'etichetta "Digita <slug> per confermare" sono divise ai punti di
# interpolazione {{ var }}. I valori enum (member/owner, status) restano nel template.
orgs-page-title = Organizzazioni - Stackpit
orgs-subtitle = Le organizzazioni a cui appartieni. Passa dall'una all'altra o creane una nuova.
orgs-empty = Non fai ancora parte di alcuna organizzazione.
orgs-col-organization = Organizzazione
orgs-col-kind = Tipo
orgs-members-btn = Membri
orgs-active = Attiva
orgs-switch = Passa
orgs-create-heading = Crea organizzazione
orgs-create-desc = Diventi il proprietario. Lo slug viene derivato dal nome se lasciato vuoto.
orgs-name = Nome
orgs-slug = Slug
orgs-optional = (facoltativo)
orgs-create-submit = Crea organizzazione

# --- Pagina membri ---
orgs-members-title-suffix = Membri - Stackpit
orgs-members-word = Membri
orgs-organization-word = organizzazione
orgs-slug-heading = Slug
orgs-slug-desc = Identifica questa organizzazione negli URL. Deve essere univoco.
orgs-email = Email
orgs-role = Ruolo
orgs-role-member = membro
orgs-role-owner = proprietario
orgs-member-fallback = utente #{ $id }
orgs-joined = Iscritto
orgs-promote = Promuovi
orgs-demote = Retrocedi
orgs-remove = Rimuovi
orgs-invites-heading = Inviti
orgs-created = Creato
orgs-expires = Scade
orgs-status = Stato
orgs-revoke = Revoca
orgs-create-invite-heading = Crea invito
orgs-create-invite-desc = Genera un link di invito utilizzabile una sola volta.
orgs-expiry-label = Scadenza (secondi)
orgs-expiry-hint = (facoltativo, predefinito 7 giorni)
orgs-create-invite-submit = Crea invito
orgs-forseti-note = L'appartenenza a questa organizzazione è gestita esternamente.
orgs-personal-note = Questa è un'organizzazione personale. L'appartenenza non è configurabile.
orgs-danger-heading = Zona pericolosa
orgs-delete-danger-pre = L'eliminazione rimuove
orgs-delete-danger-projects = progetto/i,
orgs-delete-danger-members = membro/i
orgs-delete-danger-rest = e tutti gli eventi, problemi, chiavi, avvisi e integrazioni. L'operazione non può essere annullata.
orgs-confirm-type-pre = Digita
orgs-confirm-type-post = per confermare
orgs-delete-confirm = Eliminare questa organizzazione e TUTTI i suoi dati. L'operazione non può essere annullata.
orgs-delete-submit = Elimina organizzazione

# --- Accettazione invito (pagina autonoma) ---
invite-page-title = Invito all'organizzazione - Stackpit
invite-heading = Invito all'organizzazione
invite-back-projects = Torna ai progetti
invite-intro-pre = Sei stato invitato a unirti a
invite-intro-as = come
invite-intro-post = .
invite-accept-btn = Accetta invito
invite-decline = Rifiuta
invite-error-accepted = Questo invito è già stato accettato.
invite-error-expired = Questo invito è scaduto.

# Messaggi di validazione/errore renderizzati nella pagina html_error, tradotti
# nei punti di chiamata che dispongono del locale di richiesta. Gli errori 5xx
# interni restano in inglese.
orgs-err-name-required = Il nome dell'organizzazione è obbligatorio.
orgs-err-slug-taken = Questo slug è già in uso.
orgs-err-invite-not-found = Invito non trovato o non valido.
orgs-err-org-not-found = Organizzazione non trovata.
orgs-err-last-owner-remove = L'ultimo proprietario non può essere rimosso.
orgs-err-last-owner-demote = L'ultimo proprietario non può essere retrocesso.
orgs-err-confirm-slug = Digita lo slug dell'organizzazione per confermare l'eliminazione.
orgs-err-not-deletable = Questa organizzazione non può essere eliminata.
orgs-err-limit-reached = { $count ->
    [one] Hai raggiunto il limite di { $count } organizzazione.
   *[other] Hai raggiunto il limite di { $count } organizzazioni.
}
