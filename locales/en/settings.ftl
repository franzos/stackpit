# Settings surface: the browser-defaults page (templates/browser_defaults.html,
# defaults-* keys) and the standalone org-provisioning interstitial
# (templates/provision.html, provision-* keys). Reuses nav-settings for chrome.
# Level values (fatal/error/...) stay literal in the template, matching the
# issues/events surfaces where log levels are kept as canonical English.

# --- Browser defaults ---
defaults-page-title = Browser defaults — Stackpit
defaults-heading = Browser defaults
defaults-subtitle = Set default filter values for list pages. Stored as a browser cookie.
defaults-none = No default
defaults-status-label = Default status (issues)
defaults-status-unresolved = Unresolved
defaults-status-resolved = Resolved
defaults-status-ignored = Ignored
defaults-level-label = Default level
defaults-period-label = Default time range
defaults-period-1h = Last hour
defaults-period-24h = Last 24h
defaults-period-7d = Last 7 days
defaults-period-14d = Last 14 days
defaults-period-30d = Last 30 days
defaults-period-90d = Last 90 days
defaults-period-365d = Last 365 days
defaults-save = Save defaults
defaults-clear-confirm = Clear all browser defaults?
defaults-clear = Clear all defaults
flash-defaults-saved = Defaults saved
flash-defaults-cleared = Defaults cleared

# --- Preferred language ---
settings-language-heading = Preferred language
settings-language-subtitle = Choose the language for the Stackpit interface. Signed-in accounts keep this across devices.
settings-language-label = Language
settings-language-save = Save language

settings-aria-sections = Settings sections

# --- Provisioning interstitial (standalone page) ---
provision-page-title = Set up organisations — Stackpit
provision-heading = Set up organisations
provision-subtitle-1 = The following organisations are available from your identity provider.
provision-subtitle-2 = Select the ones you want to create in Stackpit.
provision-create = Create selected
provision-skip = Skip
