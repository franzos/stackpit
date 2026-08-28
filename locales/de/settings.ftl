# Einstellungen-Oberfläche: die Browser-Standardwerte (templates/browser_defaults.html,
# defaults-*-Schlüssel) und die eigenständige Org-Provisionierungsseite
# (templates/provision.html, provision-*-Schlüssel). Nutzt nav-settings.
# Level-Werte (fatal/error/...) bleiben im Template unübersetzt, wie auf den
# Fehler-/Event-Oberflächen, wo Log-Level als kanonisches Englisch erhalten bleiben.

# --- Browser-Standardwerte ---
defaults-page-title = Browser-Standardwerte — Stackpit
defaults-subtitle = Standard-Filterwerte für Listenseiten festlegen. Wird als Browser-Cookie gespeichert.
defaults-none = Kein Standard
defaults-status-label = Standardstatus (Fehler)
defaults-status-unresolved = Ungelöst
defaults-status-resolved = Gelöst
defaults-status-ignored = Ignoriert
defaults-level-label = Standard-Level
defaults-period-label = Standard-Zeitraum
defaults-save = Standardwerte speichern
defaults-clear-confirm = Alle Browser-Standardwerte löschen?
defaults-clear = Alle Standardwerte löschen
flash-defaults-saved = Standardwerte gespeichert
flash-defaults-cleared = Standardwerte gelöscht

# --- Bevorzugte Sprache ---
settings-language-heading = Bevorzugte Sprache
settings-language-subtitle = Wähle die Sprache der Stackpit-Oberfläche. Bei angemeldeten Konten gilt sie geräteübergreifend.
settings-language-label = Sprache
settings-language-save = Sprache speichern

settings-aria-sections = Einstellungsbereiche

# --- Provisionierungsseite (eigenständige Seite) ---
provision-page-title = Organisationen einrichten — Stackpit
provision-heading = Organisationen einrichten
provision-subtitle-1 = Die folgenden Organisationen sind über deinen Identitätsanbieter verfügbar.
provision-subtitle-2 = Wähle die aus, die du in Stackpit erstellen möchtest.
provision-create = Ausgewählte erstellen
provision-skip = Überspringen

# Zustellwarteschlange
queue-page-title = Zustellwarteschlange — Stackpit
queue-subtitle = Benachrichtigungen, die nicht zugestellt werden konnten. Sie werden 24 Stunden lang automatisch wiederholt und warten danach hier auf dich.
queue-count-pending = { $count } ausstehend
queue-count-failed = { $count } fehlgeschlagen
queue-empty = Nichts in der Warteschlange.
queue-col-integration = Integration
queue-col-project = Projekt
queue-col-alert = Meldung
queue-col-state = Status
queue-col-attempts = Versuche
queue-col-queued = Eingereiht
queue-col-error = Letzter Fehler
queue-state-pending = Wird wiederholt
queue-state-failed = Aufgegeben
queue-replay = Erneut senden
queue-cancel = Verwerfen
queue-cancel-confirm = Diese Benachrichtigung verwerfen, ohne sie zuzustellen?
