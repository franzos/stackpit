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
