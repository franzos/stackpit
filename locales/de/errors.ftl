# Deutsche Entsprechungen zu locales/en/errors.ftl. Der Markenname "Stackpit"
# bleibt in den Templates woertlich, wie in base.html/login.html.
error-page-title = Fehler - Stackpit
error-heading = Fehler
error-not-found = Die angeforderte Seite existiert nicht.
error-back-projects = Zurück zu den Projekten

# Bestaetigungsseite fuer erstellte Einladungen (nur Englisch/Standard-Locale).
invite-created-page-title = Einladung erstellt - Stackpit
invite-created-heading = Einladung erstellt
invite-created-share = Teile diesen Link. Er ist { $ttl } gültig und nur einmal verwendbar.
invite-created-back-members = Zurück zu den Mitgliedern

# --- Flash-, Erfolgs- und Validierungsmeldungen (locale-abhängig) ---

# Nicht-gefunden-Diagnosen. Das Präfix "Fehler:" wird in Rust vorangestellt; der
# Wert trägt nur die Entität samt Id.
flash-not-found-project = Projekt nicht gefunden: { $id }
flash-not-found-key = Schlüssel nicht gefunden: { $id }
flash-not-found-integration = Integration nicht gefunden: { $id }
flash-not-found-alert-rule = Benachrichtigungsregel nicht gefunden: { $id }
flash-not-found-digest-schedule = Digest-Zeitplan nicht gefunden: { $id }
flash-not-found-repo = Repository nicht gefunden: { $id }
flash-not-found-project-integration = Projekt-Integration nicht gefunden: { $id }
flash-not-found-filter = { $label } nicht gefunden

# Validierung der Filterregeln
flash-unrecognized-field = Unbekanntes Feld: { $value }
flash-unrecognized-operator = Unbekannter Operator: { $value }
flash-unrecognized-action = Unbekannte Aktion: { $value }

# Projekteinstellungen
flash-project-name-updated = Projektname aktualisiert
flash-project-name-too-long = Projektname überschreitet die maximale Länge von { $max } Zeichen
flash-repo-url-required = Repository-URL ist erforderlich
flash-repo-url-too-long = Repository-URL überschreitet die maximale Länge von 2048 Zeichen
flash-repo-added = Repository hinzugefügt
flash-repo-removed = Repository entfernt
flash-project-archived = Projekt archiviert
flash-project-unarchived = Projekt dearchiviert
flash-key-created = Schlüssel erstellt
flash-key-deleted = Schlüssel gelöscht

# Benachrichtigungen und Digests
flash-project-not-found-or-denied = Fehler: Projekt nicht gefunden oder Zugriff verweigert
flash-alert-rule-created = Benachrichtigungsregel erstellt
flash-alert-rule-deleted = Benachrichtigungsregel gelöscht
flash-digest-schedule-created = Digest-Zeitplan erstellt
flash-digest-schedule-deleted = Digest-Zeitplan gelöscht

# Projekt-Integrationen
flash-integration-not-found = Integration nicht gefunden
flash-integration-activated = Integration aktiviert
flash-integration-updated = Integration aktualisiert
flash-integration-deactivated = Integration deaktiviert

# Organisations-Integrationen
flash-name-required = Name ist erforderlich
flash-invalid-integration-kind = Ungültiger Integrationstyp
flash-invalid-email-provider = Ungültiger E-Mail-Anbieter
flash-api-token-required = API-Token ist erforderlich.
flash-from-address-required = Absenderadresse ist erforderlich.
flash-smtp-not-configured = SMTP ist nicht konfiguriert. Setze [email] host in der Serverkonfiguration.
flash-invalid-to-address = Der Empfänger muss eine gültige E-Mail-Adresse sein.
flash-test-digest-sent = Test-Digest für { $count } Projekt(e) an ihre Digest-fähigen Integrationen eingereiht.
flash-test-digest-sample = Keine aktuelle Aktivität, daher wurde ein als Beispiel gekennzeichneter Digest eingereiht.
flash-test-digest-no-target = Keine Integration hat Digests für das Projekt dieses Zeitplans aktiviert.
flash-url-required = URL ist erforderlich
flash-secret-not-configured = Secret kann nicht gespeichert werden: Verschlüsselung ist nicht konfiguriert. Setze STACKPIT_MASTER_KEY, um die Speicherung von Secrets zu aktivieren.
flash-integration-license-required = Slack-, Webhook- und Issue-Tracker-Integrationen erfordern eine aktive kommerzielle Lizenz. E-Mail-Benachrichtigungen bleiben auch ohne Lizenz verfügbar.
flash-integration-created = Integration erstellt
flash-integration-name-exists = Eine Integration mit diesem Namen existiert bereits.
flash-integration-deleted = Integration gelöscht
flash-integration-no-url = Für die Integration ist keine URL konfiguriert
flash-test-notification-sent = Testbenachrichtigung gesendet

# Eingangsfilter
flash-inbound-filters-updated = Eingangsfilter aktualisiert
flash-pattern-required = Muster ist erforderlich
flash-message-filter-added = Nachrichtenfilter hinzugefügt
flash-message-filter-removed = Nachrichtenfilter entfernt
flash-rate-limit-updated = Ratenbegrenzung aktualisiert
flash-environment-required = Umgebung ist erforderlich
flash-environment-excluded = Umgebung ausgeschlossen
flash-environment-filter-removed = Umgebungsfilter entfernt
flash-release-filter-added = Release-Filter hinzugefügt
flash-release-filter-removed = Release-Filter entfernt
flash-ua-filter-added = User-Agent-Filter hinzugefügt
flash-ua-filter-removed = User-Agent-Filter entfernt
flash-rule-added = Regel hinzugefügt
flash-rule-removed = Regel entfernt
flash-cidr-required = CIDR ist erforderlich
flash-invalid-cidr = Ungültiges CIDR-Format
flash-ip-block-added = IP-Sperre hinzugefügt
flash-ip-block-removed = IP-Sperre entfernt

# Neues Projekt
flash-project-name-required = Projektname ist erforderlich
flash-integration-saved = Integration aktualisiert
flash-integration-global-not-for-trackers = Issue-Tracker nutzen keine organisationsweite Zustellung; in welches Repository geschrieben wird, ergibt sich aus den Repository-Einstellungen des jeweiligen Projekts.
flash-project-excluded = Projekt von dieser Integration ausgeschlossen
flash-project-included = Projekt nicht mehr ausgeschlossen
flash-global-email-needs-recipient = Eine organisationsweite E-Mail-Integration braucht einen Standardempfänger; Projekte, die sie nie aktiviert haben, haben keine eigene Adresse.
flash-queue-item-not-found = Wartende Benachrichtigung nicht gefunden
flash-queue-replayed = Benachrichtigung zugestellt und aus der Warteschlange entfernt
flash-queue-replay-failed = Erneutes Senden fehlgeschlagen: { $error }
flash-queue-replay-failed-generic = Erneutes Senden fehlgeschlagen. Der Grund steht am wartenden Eintrag unter „Fehler".
flash-queue-cancelled = Wartende Benachrichtigung verworfen

# Lizenzaktivierung
flash-license-activated = Lizenz aktiviert
flash-license-deactivated = Lizenz entfernt
flash-license-persist-failed = Die Lizenz wurde geprüft, konnte aber nicht gespeichert werden. Sieh ins Serverprotokoll.
flash-license-clear-failed = Die Lizenz konnte nicht entfernt werden. Sieh ins Serverprotokoll.
flash-license-empty = Füge deinen Lizenzschlüssel ein, um zu aktivieren.
flash-license-bad-signature = Diese Lizenz gilt nicht für diese Installation. Prüfe, ob du den richtigen Schlüssel eingefügt hast.
flash-license-wrong-product = Diese Lizenz ist nicht für Stackpit.
flash-license-unreadable = Diese Lizenz konnte nicht gelesen werden. Bitte prüfe sie und versuch es erneut.
flash-email-not-configured = E-Mail ist nicht konfiguriert. Füge der Serverkonfiguration einen [email]-Abschnitt mit einem Anbieter hinzu.
