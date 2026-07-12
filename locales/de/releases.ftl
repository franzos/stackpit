# Release-Oberfläche: die projektübergreifende Release-Liste und die
# projektbezogene Release-Zustandsseite. Nutzt nav-releases und nav-health
# wieder. Zählstrings nutzen tv_count-Plurale ([one]/[other]).

# --- Titel-Suffix ---
releases-title-suffix = — Stackpit

# --- Release-Liste ---
releases-list-search-placeholder = Releases durchsuchen…
releases-list-search-label = Releases durchsuchen
releases-list-project-placeholder = Projekt-ID
releases-list-project-label = Nach Projekt filtern
releases-list-period-label = Adoptionszeitraum
releases-list-period-24h = Letzte 24 Std.
releases-list-period-7d = Letzte 7 Tage
releases-list-period-30d = Letzte 30 Tage
releases-filter-submit = Filtern
releases-list-empty = Noch keine Releases. Setze ein <code class="text-mono">release</code> in deinem SDK, dann erscheinen sie hier, sobald Ereignisse eintreffen.
releases-col-version = Version
releases-col-project = Projekt
releases-col-issues = Fehler
releases-col-events = Ereignisse
releases-col-adoption = Verbreitung
releases-col-first-seen = Zuerst gesehen
releases-col-last-seen = Zuletzt gesehen

# --- Seitennavigation ---
releases-pagination-label = Seitennavigation
releases-pagination-prev = « Zurück
releases-pagination-next = Weiter »
releases-count = { $count ->
    [one] { $count } Release
   *[other] { $count } Releases
}

# --- Release-Zustand ---
release-health-title = Release-Zustand
release-health-heading = Release-Zustand
release-health-sessions-heading = Sitzungen im Zeitverlauf
release-health-empty = Keine Sitzungsdaten verfügbar. Sitzungsereignisse mit einem <code class="text-mono">status</code>-Feld erscheinen hier.
release-health-col-release = Release
release-health-col-sessions = Sitzungen
release-health-col-ok = OK
release-health-col-crashed = Abgestürzt
release-health-col-errored = Fehlerhaft
release-health-col-crash-free-sessions = Absturzfreie Sitzungen
release-health-col-crash-free-users = Absturzfreie Nutzer
release-health-subtitle = Sitzungsergebnisse sind vom SDK gemeldete Zustandssignale, keine Fehlerereignisse. Klicke auf ein Release, um seine Fehler zu sehen.
release-health-crashed-title = Fehler dieses Releases anzeigen
release-health-errored-title = Fehler dieses Releases anzeigen

# --- Release-Detail (pro Version) ---
release-detail-sessions-heading = Sitzungszustand
release-detail-sessions-note = Vom SDK gemeldete Sitzungsergebnisse (ok / fehlerhaft / abgestürzt). Das sind Zustandssignale, keine einzelnen Fehlerereignisse.
release-detail-no-health = Keine Sitzungsdaten für dieses Release.
release-detail-issues-heading = Fehler in diesem Release
release-detail-issues-note = Eigenständige Fehlergruppen, die zuerst oder zuletzt mit diesem Release gesehen wurden.
release-detail-no-issues = Keine Fehler für dieses Release erfasst.
release-health-na = n/a
