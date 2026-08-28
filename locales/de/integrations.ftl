# Integrationen-Oberfläche: die Liste (templates/integrations.html) und die drei
# Formulare zum Hinzufügen (Webhook, Slack, E-Mail). Nutzt nav-settings und
# nav-integrations für die Chrome-Elemente. Trennzeichen stehen im Template.
# integrations-empty enthält Inline-<strong>-Markup und das Pfeil-Zeichen und
# wird mit |safe gerendert.
integrations-page-title = Integrationen — Stackpit
integrations-subtitle = Ausgänge für Webhook, Slack und E-Mail. Das projektbezogene Routing wird in den Einstellungen jedes Projekts festgelegt.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + E-Mail
integrations-license-required-badge = Lizenz nötig
integrations-empty = Noch keine Integrationen. Füge oben eine hinzu, um Benachrichtigungen zu erhalten. Aktiviere sie danach pro Projekt unter <strong>Projekteinstellungen → Integrationen</strong>.
integrations-col-name = Name
integrations-col-type = Typ
integrations-col-endpoint = Endpunkt
integrations-col-created = Erstellt
integrations-delete-confirm = Diese Integration löschen? Sie wird aus allen Projekten entfernt.
integrations-test = Testen
integrations-delete = Löschen
flash-test-failed = Test fehlgeschlagen: { $error }

# Gemeinsame Formularbeschriftungen/Schaltflächen der drei Formulare.
integrations-cancel = Abbrechen
integrations-optional = (optional)
integrations-required = (erforderlich)
integrations-create = Integration erstellen

# --- Webhook hinzufügen ---
integrations-webhook-title = Webhook hinzufügen — Stackpit
integrations-webhook-breadcrumb = Webhook hinzufügen
integrations-webhook-heading = Webhook-Integration hinzufügen
integrations-webhook-name-placeholder = z. B. Produktions-Warnungen
integrations-webhook-url-label = Webhook-URL
integrations-webhook-secret-label = HMAC-Secret
integrations-webhook-secret-placeholder = Optionales Signatur-Secret

# --- Slack hinzufügen ---
integrations-slack-title = Slack hinzufügen — Stackpit
integrations-slack-breadcrumb = Slack hinzufügen
integrations-slack-heading = Slack-Integration hinzufügen
integrations-slack-name-placeholder = z. B. #alerts-Kanal
integrations-slack-url-label = Slack-Webhook-URL

# --- E-Mail hinzufügen ---
integrations-email-title = E-Mail hinzufügen — Stackpit
integrations-email-breadcrumb = E-Mail hinzufügen
integrations-email-heading = E-Mail-Integration hinzufügen
integrations-email-name-placeholder = z. B. Team-E-Mail-Warnungen
integrations-email-lock-pre = Anbieter und Absender stammen aus der
integrations-email-lock-post = Konfiguration des Servers; diese Integration wählt nur den Empfänger.
integrations-email-provider-label = Anbieter
integrations-email-token-label = API-Token
integrations-email-token-placeholder-default = Leer lassen, um den Standard zu verwenden
integrations-email-token-placeholder = API-Token des Anbieters
integrations-email-from-label = Absenderadresse
integrations-email-fromname-label = Absendername
integrations-email-smtp-hint = SMTP nutzt die [email]-Verbindung des Servers; ein Token pro Integration ist nicht nötig.
integrations-tracker-target-help = In welches Repository geschrieben wird, ergibt sich aus den Repository-Einstellungen des jeweiligen Projekts und wird deshalb nicht hier konfiguriert. Lege das Repository in den Projekteinstellungen an.
integrations-global-label = An alle Projekte zustellen
integrations-global-help = Meldungen gehen an jedes Projekt dieser Organisation, außer an die, die du auf der Seite dieser Integration ausschließt. Level- und Umgebungsfilter pro Projekt gelten weiterhin zusätzlich.
integrations-global-badge = organisationsweit
integrations-global-save = Zustellung speichern
integrations-global-on = Organisationsweit zustellen
integrations-global-off = Organisationsweite Zustellung beenden

# Integrationsdetail: Zustellung pro Projekt
integrations-detail-title = Integration — Stackpit
integrations-back = Zurück zu den Integrationen
integrations-projects-heading = Zustellung pro Projekt
integrations-projects-hint-global = Diese Integration stellt an jedes Projekt unten zu, sofern du es nicht ausschließt. Ausschließen ist die einzige Abmeldung; eine Einschlussliste gibt es nicht.
integrations-projects-hint-per-project = Diese Integration stellt nur dort zu, wo ein Projekt sie aktiviert hat. Markiere sie als organisationsweit, um überall zuzustellen.
integrations-projects-hint-tracker = Issue-Tracker werden über Forge und Host mit den Repositories eines Projekts abgeglichen. Ein ausgeschlossenes Projekt bietet diesen Tracker nicht mehr zur Auswahl an.
integrations-projects-empty = Diese Organisation hat noch keine Projekte.
integrations-summary-delivering = { $count ->
    [one] { $count } stellt zu
   *[other] { $count } stellen zu
}
integrations-summary-excluded = { $count ->
    [one] { $count } ausgeschlossen
   *[other] { $count } ausgeschlossen
}
integrations-summary-inert = { $count ->
    [one] { $count } ohne Zustellung
   *[other] { $count } ohne Zustellung
}
integrations-search-placeholder = Nach Projektnamen filtern
integrations-search-label = Projekte filtern
integrations-search-submit = Filtern
integrations-sort-label = Projekte sortieren
integrations-sort-state = Zustellende zuerst
integrations-sort-name = Nach Name
integrations-pagination-label = Seiten der Projektzustellung
integrations-projects-count = { $count ->
    [one] { $count } Projekt
   *[other] { $count } Projekte
}
integrations-col-project = Projekt
integrations-col-state = Status
integrations-project-archived = archiviert
integrations-state-default = Stellt zu
integrations-state-customised = Angepasst
integrations-state-excluded = Ausgeschlossen
integrations-state-no-repo = Kein passendes Repository
integrations-state-not-routed = Nicht aktiviert
integrations-exclude = Ausschließen
integrations-include = Einschließen
integrations-email-to-label = Standardempfänger
integrations-email-to-help = Wird verwendet, wo ein Projekt keine eigene Empfängeradresse gesetzt hat. Für organisationsweite Integrationen erforderlich.

# Issue-Tracker
integrations-add-tracker = + Issue-Tracker
integrations-tracker-title = Issue-Tracker hinzufügen — Stackpit
integrations-tracker-breadcrumb = Issue-Tracker hinzufügen
integrations-tracker-heading = Issue-Tracker-Integration hinzufügen
integrations-tracker-kind-label = Tracker
integrations-tracker-name-placeholder = z. B. GitHub Issues
integrations-tracker-url-label = Basis-URL
integrations-tracker-token-label = API-Token
integrations-tracker-token-placeholder = Persönlicher Zugriffstoken
