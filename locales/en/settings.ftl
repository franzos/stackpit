# Settings surface: the browser-defaults page (templates/browser_defaults.html,
# defaults-* keys) and the standalone org-provisioning interstitial
# (templates/provision.html, provision-* keys). Reuses nav-settings for chrome.
# Level values (fatal/error/...) stay literal in the template, matching the
# issues/events surfaces where log levels are kept as canonical English.

# --- Browser defaults ---
defaults-page-title = Browser defaults — Stackpit
defaults-subtitle = Set default filter values for list pages. Stored as a browser cookie.
defaults-none = No default
defaults-status-label = Default status (issues)
defaults-status-unresolved = Unresolved
defaults-status-resolved = Resolved
defaults-status-ignored = Ignored
defaults-level-label = Default level
defaults-period-label = Default time range
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

# Delivery queue
queue-page-title = Delivery queue — Stackpit
queue-subtitle = Notifications that could not be delivered. They retry automatically for 24 hours, then wait here for you.
queue-count-pending = { $count } pending
queue-count-failed = { $count } failed
# Just the fact. An empty queue also follows from cancelling an item or from the
# retention sweep, so "every notification has been delivered" was not true.
queue-empty = Nothing queued.
queue-col-integration = Integration
queue-col-project = Project
queue-col-alert = Alert
queue-col-state = State
queue-col-attempts = Attempts
queue-col-queued = Queued
queue-col-error = Last error
queue-state-pending = Retrying
queue-state-failed = Gave up
queue-replay = Replay
queue-cancel = Cancel
queue-cancel-confirm = Discard this notification without delivering it?
