# Organisationen-Oberfläche: die Org-Liste (templates/orgs.html), die
# Mitglieder-/Einladungsseite (templates/org_members.html) und die eigenständige
# Einladungsseite (templates/invite_accept.html, invite-*-Schlüssel). Nutzt
# nav-organizations und common-action-save. Trennzeichen stehen im Template. Der
# Lösch-Warnsatz und die Bestätigungsbeschriftung sind an ihren {{ var }}-Stellen
# geteilt. Enum-Werte (member/owner, Status) bleiben im Template unübersetzt.
orgs-page-title = Organisationen - Stackpit
orgs-subtitle = Die Organisationen, denen du angehörst. Wechsle zwischen ihnen oder erstelle eine neue.
orgs-empty = Du bist noch in keiner Organisation Mitglied.
orgs-col-organization = Organisation
orgs-col-kind = Art
orgs-members-btn = Mitglieder
orgs-active = Aktiv
orgs-switch = Wechseln
orgs-create-heading = Organisation erstellen
orgs-create-desc = Du wirst zum Inhaber. Der Slug wird aus dem Namen abgeleitet, wenn er leer bleibt.
orgs-name = Name
orgs-slug = Slug
orgs-optional = (optional)
orgs-create-submit = Organisation erstellen

# --- Mitglieder-Seite ---
orgs-members-title-suffix = Mitglieder - Stackpit
orgs-members-word = Mitglieder
orgs-organization-word = Organisation
orgs-slug-heading = Slug
orgs-slug-desc = Identifiziert diese Organisation in URLs. Muss eindeutig sein.
orgs-email = E-Mail
orgs-role = Rolle
orgs-role-member = Mitglied
orgs-role-owner = Eigentümer
orgs-member-fallback = Nutzer #{ $id }
orgs-joined = Beigetreten
orgs-promote = Befördern
orgs-demote = Zurückstufen
orgs-remove = Entfernen
orgs-invites-heading = Einladungen
orgs-created = Erstellt
orgs-expires = Läuft ab
orgs-status = Status
orgs-revoke = Widerrufen
orgs-create-invite-heading = Einladung erstellen
orgs-create-invite-desc = Erzeugt einen einmalig verwendbaren Einladungslink.
orgs-expiry-label = Ablauf (Sekunden)
orgs-expiry-hint = (optional, Standard 7 Tage)
orgs-create-invite-submit = Einladung erstellen
orgs-forseti-note = Die Mitgliedschaft dieser Organisation wird extern verwaltet.
orgs-personal-note = Dies ist eine persönliche Organisation. Die Mitgliedschaft ist nicht konfigurierbar.
orgs-danger-heading = Gefahrenzone
orgs-delete-danger-pre = Beim Löschen werden
orgs-delete-danger-projects = Projekt(e),
orgs-delete-danger-members = Mitglied(er)
orgs-delete-danger-rest = sowie alle Events, Issues, Keys, Alerts und Integrationen entfernt. Dies kann nicht rückgängig gemacht werden.
orgs-confirm-type-pre = Gib
orgs-confirm-type-post = zur Bestätigung ein
orgs-delete-confirm = Diese Organisation und ALLE ihre Daten löschen. Dies kann nicht rückgängig gemacht werden.
orgs-delete-submit = Organisation löschen

# --- Einladung annehmen (eigenständige Seite) ---
invite-page-title = Organisationseinladung - Stackpit
invite-heading = Organisationseinladung
invite-back-projects = Zurück zu den Projekten
invite-intro-pre = Du wurdest eingeladen:
invite-intro-as = als
invite-intro-post = .
invite-accept-btn = Einladung annehmen
invite-decline = Ablehnen
invite-error-accepted = Diese Einladung wurde bereits angenommen.
invite-error-expired = Diese Einladung ist abgelaufen.
invite-error-email-mismatch = Diese Einladung gilt für eine andere E-Mail-Adresse. Bitte um eine Einladung ohne E-Mail-Bindung oder melde dich mit dem passenden Konto an.

# Validierungs-/Fehlermeldungen für die html_error-Seite, an den Aufrufstellen
# mit Request-Locale übersetzt. Interne 5xx-Fehler bleiben englisch.
orgs-err-name-required = Organisationsname ist erforderlich.
orgs-err-slug-taken = Dieser Slug ist bereits vergeben.
orgs-err-invite-not-found = Einladung nicht gefunden oder ungültig.
orgs-err-org-not-found = Organisation nicht gefunden.
orgs-err-last-owner-remove = Der letzte Eigentümer kann nicht entfernt werden.
orgs-err-last-owner-demote = Der letzte Eigentümer kann nicht herabgestuft werden.
orgs-err-confirm-slug = Gib den Slug der Organisation zur Bestätigung des Löschens ein.
orgs-err-not-deletable = Diese Organisation kann nicht gelöscht werden.
orgs-err-license-cap-reached = Das Organisationslimit deiner Lizenz ist erreicht. Entferne eine Organisation oder erweitere die Lizenz, um eine weitere anzulegen.
orgs-err-limit-reached = { $count ->
    [one] Du hast das Limit von { $count } Organisation erreicht.
   *[other] Du hast das Limit von { $count } Organisationen erreicht.
}
