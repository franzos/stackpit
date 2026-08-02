# Projekt-Oberfläche: Liste, Neu, Einstellungen (Allgemein/Keys/Sourcemaps/
# Filter), Integrationen und die Erstellungs-Bestätigung. Werte mit |safe
# enthalten Inline-HTML; die Markup-Tags bleiben identisch, nur der Text ist
# übersetzt.

# --- Projektliste ---
projects-list-title = Projekte — Stackpit
projects-list-heading = Projekte
projects-list-subtitle = Überwache den Zustand deiner gesamten Architektur.
projects-list-all-events = Alle Ereignisse
projects-list-all-releases = Alle Releases
projects-list-new = + Neues Projekt
projects-list-search-placeholder = Projekte nach Name, Plattform oder Eigentümer durchsuchen…
projects-list-search-label = Projekte suchen
projects-list-filter = Filtern
projects-org-filter-label = Nach Organisation filtern
projects-org-filter-all = Alle Organisationen
projects-list-empty = Keine Projekte gefunden. Ereignisse erscheinen hier, sobald sie eingehen.
projects-period-label = Zeitraum
projects-period-all = Gesamter Zeitraum
projects-period-1h = Letzte Stunde
projects-period-24h = Letzte 24 Stunden
projects-period-7d = Letzte 7 Tage
projects-period-14d = Letzte 14 Tage
projects-period-30d = Letzte 30 Tage
projects-period-90d = Letzte 90 Tage
projects-period-365d = Letzte 365 Tage
projects-col-project = Projekt
projects-col-platforms = Plattformen
projects-col-issues = Fehler
projects-col-events = Ereignisse
projects-col-breakdown = Aufschlüsselung
projects-col-release = Release
projects-col-first-seen = Zuerst gesehen
projects-col-last-seen = Zuletzt gesehen
projects-breakdown-errors = Fehler:
projects-breakdown-transactions = Transaktionen:
projects-breakdown-sessions = Sitzungen:
projects-breakdown-other = Sonstige:
projects-legend-errors = Fehler
projects-legend-transactions = Transaktionen
projects-legend-sessions = Sitzungen
projects-legend-other = Sonstige

# --- Gemeinsam in Projektformularen ---
projects-optional = (optional)
projects-cancel = Abbrechen
projects-remove = Entfernen
projects-delete = Löschen
projects-name-placeholder = Mein Projekt

# --- Neues Projekt ---
projects-new-title = Neues Projekt — Stackpit
projects-new-heading = Neues Projekt
projects-new-name-label = Projektname
projects-new-platform-label = Plattform
projects-new-platform-select = Plattform auswählen…
projects-new-platform-other = Andere
projects-new-platform-native = Native (C/C++)
projects-new-submit = Projekt erstellen

# --- Einstellungen-Tabs (von den Einstellungsseiten geteilt) ---
projects-tab-general = Allgemein
projects-tab-sdk = SDK-Einrichtung
projects-tab-sourcemaps = Source-Maps
projects-tab-filters = Filter
projects-tab-integrations = Integrationen

# --- Einstellungen: Allgemein ---
projects-settings-heading = Einstellungen
projects-settings-archived = (archiviert)
projects-settings-name-heading = Projektname
projects-settings-display-name = Anzeigename
projects-settings-save-name = Namen speichern
projects-settings-info-heading = Projektinfo
projects-settings-status = Status
projects-settings-source = Quelle
projects-repos-heading = Quell-Repositories
projects-repos-help = Verknüpfe Stack-Frames mit dem Quellcode auf deiner Forge. Registriere ein Release mit einem Commit-SHA über <code class="text-mono">sentry-cli</code>, um Links zu aktivieren.
projects-repos-empty = Keine Repositories konfiguriert.
projects-repos-url-label = Repository-URL
projects-repos-col-forge = Forge
projects-repos-template = URL-Vorlage
projects-repos-auto = automatisch
projects-repos-remove-confirm = Dieses Repository entfernen?
projects-repos-add = Repository hinzufügen
projects-repos-add-help = Fügt anklickbare Quell-Links (z. B. "Auf GitHub ansehen") neben Stack-Frames hinzu. Erfordert ein Release mit einem Commit-SHA — der Forge-Typ wird automatisch erkannt. Unterstützt: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps. Für andere Forges eine URL-Vorlage angeben.
projects-danger-heading = Gefahrenzone
projects-archive-desc = Dieses Projekt archivieren. Archivierte Projekte lehnen neue Ereignisse ab.
projects-archive-confirm = Dieses Projekt archivieren? Neue Ereignisse werden abgelehnt.
projects-archive-submit = Projekt archivieren
projects-unarchive-desc = Dieses Projekt aus dem Archiv holen, um wieder Ereignisse anzunehmen.
projects-unarchive-submit = Projekt aus Archiv holen
projects-delete-desc = Dieses Projekt und alle seine Daten dauerhaft löschen. Das kann nicht rückgängig gemacht werden.
projects-delete-confirm = Dieses Projekt und ALLE seine Daten löschen? Das kann nicht rückgängig gemacht werden.
projects-delete-submit = Projekt löschen
projects-move-heading = In Organisation verschieben
projects-move-desc = Dieses Projekt in eine andere Organisation verschieben, die dir gehört. Daten und DSNs bleiben gültig, aber Benachrichtigungs-Integrationen werden getrennt und müssen in der neuen Organisation neu hinzugefügt werden.
projects-move-target-label = Ziel-Organisation
projects-move-confirm-pre = Gib
projects-move-confirm-post = zur Bestätigung ein.
projects-move-confirm-placeholder = Projektname
projects-move-confirm-dialog = Dieses Projekt in die ausgewählte Organisation verschieben?
projects-move-submit = Projekt verschieben
projects-move-err-invalid-target = Ungültige Ziel-Organisation.
projects-move-err-name-mismatch = Der Projektname stimmt nicht überein.
projects-move-err-denied = Du bist kein Eigentümer der Ziel-Organisation.
projects-move-err-conflict = Das Projekt konnte nicht verschoben werden; es hat sich möglicherweise geändert. Bitte erneut versuchen.

# --- Einstellungen: SDK-Einrichtung / Keys ---
projects-keys-title = SDK-Einrichtung
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = Keine Keys registriert. Erstelle unten einen Key, um eine DSN zu erhalten.
projects-keys-list-heading = Projekt-Keys
projects-keys-empty = Keine Keys für dieses Projekt registriert.
projects-keys-col-public = Öffentlicher Key
projects-keys-col-label = Bezeichnung
projects-keys-col-status = Status
projects-keys-col-created = Erstellt
projects-keys-delete-confirm = Diesen Key löschen? SDKs, die ihn verwenden, funktionieren dann nicht mehr.
projects-keys-create-heading = Key erstellen
projects-keys-label-label = Bezeichnung
projects-keys-label-placeholder = z. B. production, staging
projects-keys-create-submit = Key erstellen

# --- Einstellungen: Source-Maps ---
projects-sourcemaps-title = Source-Maps
projects-sourcemaps-apikey-heading = API-Key
projects-sourcemaps-apikey-desc = Das Hochladen von Source-Maps erfordert einen API-Key. Nur für dieses Projekt und ausschließlich für Source-Map-Operationen nutzbar.
projects-sourcemaps-key-generated = Key erzeugt:
projects-sourcemaps-key-warning = Kopiere diesen Key jetzt — er wird nicht erneut angezeigt.
projects-sourcemaps-col-key = Schlüssel
projects-sourcemaps-regen-confirm = Key neu erzeugen? Der aktuelle Key funktioniert dann nicht mehr.
projects-sourcemaps-regen = Neu erzeugen
projects-sourcemaps-empty = Kein Source-Map-API-Key für dieses Projekt.
projects-sourcemaps-generate = Key erzeugen
projects-sourcemaps-setup-heading = Einrichtung
projects-sourcemaps-setup-desc = Verwende <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a>, um Source-Maps hochzuladen. Setze diese Umgebungsvariablen:
projects-sourcemaps-then-upload = Dann hochladen:

# --- Einstellungen: Filter ---
projects-filters-inbound-heading = Eingangsfilter
projects-filters-inbound-desc = Integrierte Filter, die Ereignisse verwerfen, die gängigen Rauschmustern entsprechen.
projects-filters-browser-ext = Browser-Erweiterungen — Ereignisse von Chrome-/Firefox-/Safari-Erweiterungen verwerfen
projects-filters-localhost = Localhost — Ereignisse von localhost, 127.0.0.1, privaten IPs verwerfen
projects-filters-inbound-submit = Eingangsfilter speichern
projects-filters-message-heading = Nachrichtenfilter
projects-filters-message-help = Glob-Muster, die gegen Ereignistitel geprüft werden. Verwende <code class="text-mono">*</code> für eine beliebige Folge, <code class="text-mono">?</code> für ein einzelnes Zeichen.
projects-filters-col-pattern = Muster
projects-filters-message-empty = Keine Nachrichtenfilter konfiguriert.
projects-filters-add-pattern = Muster hinzufügen
projects-filters-message-submit = Nachrichtenfilter hinzufügen
projects-filters-ratelimit-heading = Ratenbegrenzung
projects-filters-ratelimit-desc = Maximale Ereignisse pro Minute für dieses Projekt. 0 = unbegrenzt.
projects-filters-ratelimit-label = Ereignisse pro Minute
projects-filters-ratelimit-submit = Ratenbegrenzung speichern
projects-filters-env-heading = Ausgeschlossene Umgebungen
projects-filters-env-desc = Ereignisse aus diesen Umgebungen werden stillschweigend verworfen.
projects-filters-col-environment = Umgebung
projects-filters-env-empty = Keine ausgeschlossenen Umgebungen.
projects-filters-env-add-label = Ausgeschlossene Umgebung hinzufügen
projects-filters-env-submit = Umgebung ausschließen
projects-filters-release-heading = Release-Filter
projects-filters-release-desc = Glob-Muster, die gegen Release-Versionen geprüft werden. Passende Ereignisse werden verworfen.
projects-filters-release-empty = Keine Release-Filter.
projects-filters-release-submit = Release-Filter hinzufügen
projects-filters-ua-heading = User-Agent-Filter
projects-filters-ua-desc = Glob-Muster, die gegen User-Agent-Header geprüft werden. Integrierte Muster für kube-probe und Health-Checker sind immer aktiv.
projects-filters-ua-empty = Keine eigenen User-Agent-Filter.
projects-filters-ua-submit = User-Agent-Filter hinzufügen
projects-filters-rules-heading = Eigene Regeln
projects-filters-rules-desc = Erweiterte Regeln, die Ereignisfelder abgleichen. Regeln mit höherer Priorität werden zuerst ausgewertet.
projects-filters-col-field = Feld
projects-filters-col-operator = Operator
projects-filters-col-value = Wert
projects-filters-col-action = Aktion
projects-filters-col-priority = Priorität
projects-filters-rules-empty = Keine eigenen Regeln.
projects-filters-sample-rate-label = Abtastrate
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = Regel hinzufügen
projects-filters-op = { $op ->
    [not_equals] ungleich
    [contains] enthält
    [not_contains] enthält nicht
    [starts_with] beginnt mit
    [in] in Liste
    [not_in] nicht in Liste
   *[equals] gleich
}
projects-filters-action = { $action ->
    [sample] Stichprobe
   *[drop] verwerfen
}
projects-filters-ip-heading = IP-Sperrliste
projects-filters-ip-desc = CIDR-Blöcke oder einzelne IPs. Ereignisse von gesperrten IPs werden stillschweigend verworfen.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = Keine IP-Blöcke konfiguriert.
projects-filters-ip-add-label = CIDR hinzufügen
projects-filters-ip-submit = IP-Bereich sperren
projects-filters-discard-heading = Verwerfungs-Statistik
projects-filters-discard-window = (letzte 7 Tage)
projects-filters-col-date = Datum
projects-filters-col-reason = Grund
projects-filters-col-count = Anzahl

# Filter-Entitätsbezeichnungen, in flash-not-found-filter beim Löschen eingesetzt.
projects-filter-label-message = Nachrichtenfilter
projects-filter-label-environment = Umgebungsfilter
projects-filter-label-release = Release-Filter
projects-filter-label-user-agent = User-Agent-Filter
projects-filter-label-rule = Filterregel

# --- Einstellungen: Integrationen ---
projects-integrations-active-heading = Aktive Integrationen
projects-integrations-active-empty = Keine Integrationen aktiviert. Füge zuerst eine globale Integration auf der Seite <a class="text-primary" href="/web/settings/integrations/">Integrationen</a> hinzu und aktiviere sie dann hier. Du kannst jede nach Mindeststufe und Umgebung eingrenzen, damit Dev-Rauschen aus Prod-Kanälen fernbleibt.
projects-integrations-deactivate-confirm = Diese Integration für das Projekt deaktivieren?
projects-integrations-deactivate = Deaktivieren
projects-integrations-notify-new-issues = Neue Fehler
projects-integrations-notify-regressions = Regressionen
projects-integrations-notify-threshold = Schwellenwert-Warnungen
projects-integrations-notify-digests = Zusammenfassungen
projects-integrations-min-level = Mindeststufe
projects-integrations-level-any = Beliebig
projects-integrations-env-filter = Umgebungsfilter
projects-integrations-env-placeholder = z. B. production
projects-integrations-to-address = Empfängeradresse
projects-integrations-to-address-note = (nur E-Mail-Integrationen)
projects-integrations-activate-heading = Integration aktivieren
projects-integrations-integration-label = Integration
projects-integrations-activate-submit = Aktivieren
projects-integrations-available-empty = Keine Integrationen verfügbar. <a class="text-primary" href="/web/settings/integrations/">Erstelle zuerst eine</a>.

# --- Projekt erstellt ---
projects-created-word = erstellt
projects-created-breadcrumb = Erstellt
projects-created-heading = Projekt erstellt
projects-created-subtitle = Verwende die DSN unten, um dein SDK zu konfigurieren.
projects-created-settings-btn = Projekteinstellungen
projects-created-back = Zurück zu den Projekten
projects-created-details-heading = Projektdetails
projects-created-col-id = Projekt-ID
projects-created-sdk-desc-before = Installiere das Sentry-SDK für
projects-created-sdk-desc-after = und initialisiere es mit der obigen DSN.
projects-created-docs-javascript = Sentry JavaScript Doku →
projects-created-docs-python = Sentry Python Doku →
projects-created-docs-rust = Sentry Rust Doku →
projects-created-docs-go = Sentry Go Doku →
projects-created-docs-node = Sentry Node.js Doku →
projects-created-docs-java = Sentry Java Doku →
projects-created-docs-ruby = Sentry Ruby Doku →
projects-created-docs-php = Sentry PHP Doku →
projects-created-docs-elixir = Sentry Elixir Doku →
projects-created-docs-dotnet = Sentry .NET Doku →
projects-created-docs-apple = Sentry Apple Doku →
projects-created-docs-kotlin = Sentry Kotlin Doku →
projects-created-docs-native = Sentry Native Doku →
projects-created-docs-generic = Sentry Plattform-Doku →
