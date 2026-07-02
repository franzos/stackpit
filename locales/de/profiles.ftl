# Profil-Oberfläche: die projektbezogene Profil-Liste und die Profil-
# Detailseite. Nutzt nav-profiles wieder. Zählstrings nutzen tv_count-Plurale
# ([one]/[other]).

# --- Titel-Suffix ---
profiles-title-suffix = — Stackpit

# --- Profil-Liste ---
profiles-list-empty = Keine Profile gefunden. Profil-Ereignisse mit <code class="text-mono">item_type = "profile"</code> erscheinen hier.
profiles-col-event-id = Ereignis-ID
profiles-col-transaction = Transaktion
profiles-col-platform = Plattform
profiles-col-release = Release
profiles-col-environment = Umgebung
profiles-col-timestamp = Zeitstempel

# --- Profil-Detail ---
profiles-detail-heading = Profil
profiles-detail-raw-payload = Rohdaten

# --- Seitennavigation ---
profiles-pagination-label = Seitennavigation
profiles-pagination-prev = « Zurück
profiles-pagination-next = Weiter »
profiles-count = { $count ->
    [one] { $count } Profil
   *[other] { $count } Profile
}
