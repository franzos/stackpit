# Alerts & digests settings page (templates/alerts.html). Reuses nav-settings
# and nav-alerts-digests for chrome. Separator spaces live in the template, so
# values carry no leading/trailing whitespace. alerts-page-title keeps the raw
# &amp; entity and is rendered with |safe.
alerts-page-title = Alerts &amp; digests — Stackpit
alerts-notify-help-pre = Notifications fire through the integrations on the
alerts-notify-help-post = page.

# --- Notification types (per-integration new-issue / regression toggles) ---
alerts-notify-types-heading = Notification types
alerts-notify-types-desc = New-issue and regression alerts fire on every newly-seen or regressed issue, gated per integration below. Threshold rules fire on event volume in a window; digests are periodic summaries.
alerts-notify-types-empty = No active project integrations yet. Link one from a project's integrations page.
alerts-col-integration = Integration
alerts-col-new-issues = New issues
alerts-col-regressions = Regressions
alerts-col-digests = Digests
alerts-notify-save = Save

# --- Threshold rules ---
alerts-threshold-heading = Threshold rules
alerts-threshold-desc = Fire when an issue receives more than N events in a time window.
alerts-rules-empty = No alert rules yet.
alerts-col-scope = Scope
alerts-col-issue = Issue
alerts-col-threshold = Threshold
alerts-col-window = Window
alerts-col-cooldown = Cooldown
alerts-scope-global = Global
alerts-fingerprint-any = Any
alerts-rule-delete-confirm = Delete this alert rule?
alerts-delete-label = Delete
alerts-add-rule = + Add alert rule
alerts-all-projects = All projects
alerts-project-fallback = Project { $id }
alerts-fingerprint-label = Issue fingerprint
alerts-fingerprint-hint = (blank = any)
alerts-fingerprint-placeholder = any issue
alerts-fingerprint-help = A fingerprint identifies one issue (grouped events). Visible in the URL on any issue page. Leave blank to match every issue in scope.
alerts-unit-s = (s)
alerts-create-rule = Create rule

# --- Digest schedules ---
alerts-digest-heading = Digest schedules
alerts-digest-desc = Periodic activity summaries — daily or weekly stand-ups instead of per-event noise.
alerts-digests-empty = No digest schedules yet.
alerts-col-interval = Interval
alerts-col-last-sent = Last sent
alerts-col-enabled = Enabled
alerts-never = Never
alerts-yes = Yes
alerts-no = No
alerts-digest-delete-confirm = Delete this digest schedule?
alerts-add-digest = + Add digest schedule
alerts-interval-daily = Daily (24h)
alerts-interval-weekly = Weekly (7d)
alerts-interval-hourly = Hourly
alerts-create-schedule = Create schedule
