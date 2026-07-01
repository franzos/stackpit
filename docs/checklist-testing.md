# Stackpit testing checklist

This is the master list of end-to-end test cases for the stackpit stack — admin UI,
public ingest, both auth paths, the two API surfaces, and the security middleware.
The `e2e-review` skill (`.claude/skills/e2e-review/SKILL.md`) drives §§3–9 of this
document: the automated tiers cover §2, §5, §6, and §8; the interactive Chrome
walkthrough covers §4, §7, and §9.

Each item has a stable ID (e.g. `4.3.2`) so findings can reference it. Check items
off as you verify them. Don't fix anything mid-review — record the finding with a
`file:line` pointer and hand it back.

Cross-checked against the code on `master` (stackpit 0.3.6), after the
spans/transactions performance work and the OIDC work merged. Where this list and an
older skill note disagree, the code wins; known drifts are flagged inline with
**(was: …)**.

---

## §0 — How to use this list

- **Ports:** this list assumes the skill's `:3333` (admin) / `:3334` (ingest) split,
  matching the repo `stackpit.toml`. Substitute if your config differs.
- **Rebuild first.** A stale binary is the single biggest time-waster — symptoms are
  `unsafe-inline` in the script-src CSP, `/web/_assets/*.js` 404, a phantom CSRF
  cookie, or `/health` 404. Always `guix shell -m manifest.scm -- cargo build --release`
  before testing.
- **Two listeners, two route trees.** `/web/*`, `/api/v1/*`, admin `/health` and
  static assets live on the **admin** port. Envelope/store/security/minidump ingest,
  the Sentry-compat `/api/0/*` surface, and ingest `/health` live on the **ingest**
  port. Hitting the wrong port returns 404 and looks like a broken endpoint — it isn't.
- **CSRF is a synchronizer token**, deterministic per admin_token (HMAC-SHA256), or
  random-per-grant under OIDC, or the constant `noauth` in no-auth loopback mode. It
  is rendered as `<input type="hidden" name="csrf_token">` in every mutating form.
  There is **no CSRF cookie**.

---

## §0.5 — Per-screen UX rubric

Apply to **every** screen in §4 and §7. Each is a pass/fail observation, not a fix.

- [ ] **0.5.1 Next step obvious** — the primary action on the page is visually clear.
- [ ] **0.5.2 Error copy actionable** — failures say what went wrong and what to do, not a raw code or stack.
- [ ] **0.5.3 Success feedback** — mutations produce a visible confirmation (inline message or redirect to an updated view).
- [ ] **0.5.4 Empty states useful** — empty lists explain what lands here and how, not a blank table.
- [ ] **0.5.5 Keyboard-only** — the flow is completable with Tab/Enter; focus order is sane.
- [ ] **0.5.6 Screen-reader sanity** — headings, labels, and `aria` roles are present on forms and tablists.
- [ ] **0.5.7 Narrow viewport** — layout holds at ~375px wide (no horizontal scroll on core content).
- [ ] **0.5.8 Destructive POSTs guarded** — delete/archive/discard carry a `data-confirm` prompt.
- [ ] **0.5.9 Copy reads naturally** — no placeholder text, no dev jargon leaking to users.
- [ ] **0.5.10 Console clean** — no JS errors or CSP violations in the browser console.

---

## §1 — Prerequisites & bring-up

- [ ] **1.1 Release binary fresh** — `guix shell -m manifest.scm -- cargo build --release` completes clean.
- [ ] **1.2 Config present** — `stackpit.toml` has `[server]` + `[storage]` + `[filter] mode = "open"`. Generate with `./target/release/stackpit init` if missing.
- [ ] **1.3 Ports free** — `:3333` and `:3334` unbound (or rewrite `bind` / `ingest_bind`).
- [ ] **1.4 Clean DB + serve** — wipe `stackpit.db*`, `nohup ./target/release/stackpit serve`, confirm log shows `ingestion listening on 127.0.0.1:3334` **and** `admin listening on 127.0.0.1:3333`. A `STACKPIT_MASTER_KEY` warning is expected when no key is exported and no encrypted integrations exist.
- [ ] **1.5 Seed fake data** — `python3 scripts/generate-fake-data.py --count 500 --quiet`. Expect `~5100 ok, 0 failed` and `named 100/100 projects, 100 sourcemap keys, ~300 releases`.
- [ ] **1.6 DB counts non-zero** — `events`, `issues`, `logs`, `spans`, `metrics`, `releases` all > 0; `projects WHERE name<>''` = 100. If `logs`/`spans`/`metrics` = 0 the seed script regressed.

---

## §2 — Automated tier (cargo + curl)

Fast, fire-and-forget, no UI. This is the regression net; run it before the browser walkthrough.

### §2.1 Rust suite

- [ ] **2.1.1** `guix shell -m manifest.scm -- cargo test --no-default-features --features sqlite` — all pass. Report counts + any failing test name.
- [ ] **2.1.2 (optional)** Postgres tier: `DATABASE_URL=postgres://… cargo test --no-default-features --features postgres -- --test-threads=1`. Skip unless asked.
- [ ] **2.1.3 (optional)** Integration suite against a live server: `make serve-bg && make seed && guix shell make sqlite -- make test-integration`.
- [ ] **2.1.4 (optional)** Playwright smoke: `guix shell make -- make e2e` (login → create → issue-list → rename).

### §2.2 Login + cookie + CSRF (curl)

- [ ] **2.2.1 Login success** — `POST /web/login` with `token=<admin_token>` → 303 to `/web/projects/`.
- [ ] **2.2.2 Cookie shape** — response sets `stackpit_token` (or `__Host-stackpit_token` when cookies are secure), `HttpOnly`, `SameSite=Strict`, value = `sha256(admin_token)` (hex). No Max-Age (session cookie).
- [ ] **2.2.3 Wrong token** — `token=wrong` → 401, login form re-rendered with "Invalid token".
- [ ] **2.2.4 CSRF token rendered** — fetch a settings page with the session cookie; `csrf_token` appears as a hidden input value (64 hex chars), deterministic per admin_token.
- [ ] **2.2.5 CSRF missing** — POST a mutating form with no `csrf_token` → 403.
- [ ] **2.2.6 CSRF wrong** — POST with `csrf_token=00…00` → 403.
- [ ] **2.2.7 CSRF correct** — POST with the rendered token → 200/303.

### §2.3 JSON API auth (curl)

- [ ] **2.3.1** `GET /api/v1/projects/` with `Authorization: Bearer <admin_token>` → 200.
- [ ] **2.3.2** Same endpoints 200 with token: `/api/v1/projects/1/issues/`, `/api/v1/projects/1/events/`, `/api/v1/alerts/rules`, `/api/v1/digests`.
- [ ] **2.3.3** Wrong bearer → 401 JSON `{"error":"unauthorized"}`.

### §2.4 Static assets + health (curl)

- [ ] **2.4.1** Admin `GET /health` → 200, body `ok` (plain text).
- [ ] **2.4.2** Ingest `GET /health` → 200, body `ok` (plain text). **(was: skill claims a JSON counter blob — code returns the static string `"ok"`, `server.rs:103`.)**
- [ ] **2.4.3** `/web/_assets/style.css` → 200 `text/css`; `/web/_assets/icon.svg` → 200 `image/svg+xml`.
- [ ] **2.4.4** `/web/_assets/bulk.js`, `confirm.js`, `stop-propagation.js` → 200, `application/javascript; charset=utf-8`, `Cache-Control: public, max-age=86400`.

---

## §3 — Browser walkthrough setup

Destructive. Wipes and reseeds, then logs in via a fresh browser tab.

- [ ] **3.1 Wipe confirmed** — operator approved wiping `stackpit.db`. Re-run §1.4 + §1.5.
- [ ] **3.2 Fresh tab** — `tabs_create_mcp()` (never reuse an old tab — stale cookies confuse tests).
- [ ] **3.3 Login** — navigate to `http://127.0.0.1:3333/web/login`, submit the admin_token from `stackpit.toml`, land on `/web/projects/` (303).

---

## §4 — Admin UI surfaces

### §4.1 Home / project list

Routes: `GET /` and `GET /web/` → 308 to `/web/projects/`; `GET /web/projects/`.

- [ ] **4.1.1 Redirects** — `/` and `/web/` both 308 to `/web/projects/`.
- [ ] **4.1.2 Listing** — 100 named projects shown.
- [ ] **4.1.3 Columns** — Project, Platforms, Issues, Events, Breakdown (errors/transactions/sessions/other), Release, First Seen, Last Seen.
- [ ] **4.1.4 Filter form** — `query` (text) + `period` select (`""`/`1h`/`24h`/`7d`/`14d`/`30d`/`90d`/`365d`); period default comes from the browser-defaults cookie if set.
- [ ] **4.1.5 Sort headers** — links toggle `sort` = `project_id` / `issues` / `events` / `first_seen` (default = last seen).
- [ ] **4.1.6 Aggregate counts** — per-period Issues/Events totals reflect the selected period.
- [ ] **4.1.7 Nav links** — "New Project" → `/web/projects/new`, "All Events" → `/web/events/`, "All Releases" → `/web/releases/`.
- [ ] **4.1.8 Empty state** — with a filter that matches nothing: "No projects found. Events will appear here once ingested."

### §4.2 Create project

Routes: `GET /web/projects/new`, `POST /web/projects/new`.

- [ ] **4.2.1 Form** — `name` (text, required) + `platform` select (javascript/python/rust/go/java/node/ruby/php/elixir/csharp/swift/kotlin/native/other) + `csrf_token`.
- [ ] **4.2.2 Success** — submit → `project_created.html` shows project ID, name, public key, **full DSN**, platform. No redirect.
- [ ] **4.2.3 DSN host** — DSN host follows `external_ingest_url` → `external_url` → `http://{ingest_bind}`; confirm it points at the **ingest** port (`:3334`) for split-port config.
- [ ] **4.2.4 Empty name** — submit blank → re-render with "Project name is required".
- [ ] **4.2.5 DB row** — `SELECT project_id, name FROM projects WHERE name='…'` lands.

### §4.3 Issue list (per project)

Route: `GET /web/projects/{project_id}/`.

- [ ] **4.3.1 Defaults redirect** — absent `status`/`level`/`period` with a browser-defaults cookie → temporary redirect adding the defaults.
- [ ] **4.3.2 Tab strip** — Issues/Transactions/Spans/Logs/Metrics/Monitors/Profiles/Replays/Health/User Reports/Client Reports/Settings; zero-count tabs hidden.
- [ ] **4.3.3 Histogram** — event-count SVG renders above the filters when non-empty.
- [ ] **4.3.4 Filter form** — `query`, `status` (`""`/unresolved/resolved/ignored), `level` (`""`/fatal/error/warning/info/debug), `release` (only when releases exist), `period` (default `7d`); `sort` + `tag` carried as hidden.
- [ ] **4.3.5 Columns** — checkbox, Title (type + message split), Level, Events, Users, First Seen, Last Seen, Status.
- [ ] **4.3.6 Sort headers** — `event_count` / `first_seen` (default = last seen).
- [ ] **4.3.7 Bulk bar (selected)** — ticking a row reveals `POST .../bulk` with `mode=selected`, `ids[]` per fingerprint, actions resolve/ignore/delete.
- [ ] **4.3.8 Bulk bar (all matching)** — "Resolve/Ignore/Delete all N" form with `mode=all_matching` carrying the current filter state.
- [ ] **4.3.9 Bulk round-trip** — resolve a selection → issues move to resolved; verify `SELECT status FROM issues WHERE fingerprint='…'`.
- [ ] **4.3.10 Empty state** — filter matching nothing: "No issues match the current filters." No bulk forms rendered.

### §4.4 Issue detail

Route: `GET /web/projects/{project_id}/issues/{fingerprint}/?tab=details|events`.

- [ ] **4.4.1 Heading** — `<h1>` = error type; `.issue-title-msg` = message.
- [ ] **4.4.2 Summary tags** — clickable pills link back to the issue list filtered by that tag.
- [ ] **4.4.3 Sub-tabs** — Details (default) / All Events tablist.
- [ ] **4.4.4 Stacktrace** — "Exception & Stacktrace" `<details>` open by default; `in_app` frames coloured; filename:line:col, function, context code shown.
- [ ] **4.4.5 Other accordions** — Breadcrumbs (N), Tags (N), Contexts, Request, User Reports (N, open when present), Attachments (N), Raw JSON.
- [ ] **4.4.6 Sidebar meta** — Fingerprint, Level, Status, Events, Users, First/Last Seen (+ release links), event chart SVG, tag-facets panel.
- [ ] **4.4.7 Status form** — `POST .../status` with `status` select → 303 back; verify `SELECT status FROM issues`.
- [ ] **4.4.8 Discard form** — `POST .../discard` toggles "Discard Future Events" ⇄ "Undo Discard" (data-confirm on discard).
- [ ] **4.4.9 Events tab** — `?tab=events` paginated list links to event detail.
- [ ] **4.4.10 Attachment download** — `GET .../events/{event_id}/attachments/{filename}` forces `application/octet-stream` + `Content-Disposition: attachment`.

### §4.5 Event detail

Route: `GET /web/projects/{project_id}/events/{event_id}/`.

- [ ] **4.5.1 Heading** — `<h1>` = event title.
- [ ] **4.5.2 Event-nav strip** — Newer/Older links + total count when the event belongs to an issue with siblings.
- [ ] **4.5.3 Accordions** — same set as §4.4.5; Raw JSON always present.
- [ ] **4.5.4 Sidebar** — Event ID, Type, Timestamp, Level, Platform, Transaction, Release, Environment, Server, SDK name+version, Received; User sub-section (id/email/username/ip) when present.
- [ ] **4.5.5 Summary tags** — non-clickable spans here (unlike issue detail).
- [ ] **4.5.6 Web Vitals card** — for transaction events, a Web Vitals card (LCP, FCP, CLS, TTFB, …) renders from the transaction's measurements with per-metric ratings; absent for plain error events.

### §4.6 Per-project tabs

Each tab only appears in the nav when its count > 0. The seed exercises all of them for at least some projects.

- [ ] **4.6.1 Transactions** — `/transactions/` is a performance rollup (one row per transaction name) with `period` (default `7d`) and `sort` (default `p95`); columns surface p50/p75/p95 duration, throughput, and failure rate. No bulk actions (transactions feed the rollup, not error-style issues). `/transactions/detail?name=<txn>` lists that transaction's instances slowest-first, shows the trace op in the header and a per-instance trace status badge, and links each instance into event/trace detail.
- [ ] **4.6.2 Spans + Traces** — `/spans/` shows span list + top-25 traces; `/traces/{trace_id}/` renders the span waterfall (nested, time-positioned bars) plus correlated error events.
- [ ] **4.6.3 Logs** — `/logs/`, filter `query` + `level`.
- [ ] **4.6.4 Metrics** — `/metrics/` name list; `/metrics/{mri}` time-series chart (`from`/`to` params).
- [ ] **4.6.5 Monitors** — `/monitors/` list; `/monitors/{slug}/` check-in list; `POST /monitors/{slug}/bulk` delete.
- [ ] **4.6.6 Profiles** — `/profiles/` list; `/profiles/{event_id}/` detail with raw JSON.
- [ ] **4.6.7 Replays** — `/replays/` list; `/replays/{event_id}/` detail with raw JSON.
- [ ] **4.6.8 Health** — `/health/` per-release session table with crash-free sessions and crash-free users (distinct-user crash rate via HLL), plus a daily sessions trend chart.
- [ ] **4.6.9 User Reports** — `/user-reports/`; `POST /user-reports/bulk` delete.
- [ ] **4.6.10 Client Reports** — `/client-reports/`; `POST /client-reports/bulk` delete.
- [ ] **4.6.11 Empty states** — each tab renders a useful empty message when its list is empty.

### §4.7 Project settings

Two-level tab bar: General / SDK Setup / Source Maps / Filters / Integrations.

**General — `/settings/`**
- [ ] **4.7.1 Rename** — `POST /settings/name` (`name`, max 255) → inline success; verify `SELECT name FROM projects`.
- [ ] **4.7.2 Repos add** — `POST /settings/repos` (`repo_url` required, `url_template` optional); forge auto-detected.
- [ ] **4.7.3 Repos delete** — `POST /settings/repos/{repo_id}/delete` (data-confirm).
- [ ] **4.7.4 Archive** — `POST /settings/archive` (data-confirm) → status flips to archived (only shown when active).
- [ ] **4.7.5 Unarchive** — `POST /settings/unarchive` → status flips back (only shown when archived).
- [ ] **4.7.6 Delete** — `POST /settings/delete` (data-confirm) → 303 to `/web/projects/`; project gone from DB.

**SDK Setup / Keys — `/settings/keys/`**
- [ ] **4.7.7 DSN display** — first key's DSN in a `<pre><code>` block, or empty-state.
- [ ] **4.7.8 Create key** — `POST /settings/keys/create` (`label` optional) → new key row.
- [ ] **4.7.9 Delete key** — `POST /settings/keys/{public_key}/delete` (data-confirm).

**Source Maps — `/settings/sourcemaps/`**
- [ ] **4.7.10 Generate** — `POST /settings/sourcemaps/generate` mints an `spk_`+32-hex key, shown once with a sentry-cli upload command; verify `api_keys` row with `scope='sourcemap'`.

**Filters — `/settings/filters/`** (all 8 sub-sections round-trip)
- [ ] **4.7.11 Inbound** — `POST /filters/inbound` (`browser_extensions`, `localhost` checkboxes).
- [ ] **4.7.12 Message filters** — add (`pattern`) + delete (`/messages/{id}/delete`).
- [ ] **4.7.13 Rate limit** — `POST /filters/rate-limit` (`max_events_per_minute`, 0 = unlimited).
- [ ] **4.7.14 Excluded environments** — add (`environment`) + delete.
- [ ] **4.7.15 Release filters** — add (`pattern`) + delete.
- [ ] **4.7.16 User-agent filters** — add (`pattern`) + delete.
- [ ] **4.7.17 Custom rules** — `POST /filters/rules` (`field`/`operator`/`value`/`action`/`sample_rate`/`priority`) + delete.
- [ ] **4.7.18 IP blocklist** — add (`cidr`, CIDR-validated) + delete; invalid CIDR rejected.
- [ ] **4.7.19 Discard stats** — last-7-days table renders when data exists.

### §4.8 Global settings

**Integrations — `/web/settings/integrations/`**
- [ ] **4.8.1 List** — Name, Type (kind), URL, Created, Test + Delete per row; empty-state explains webhook/Slack/email.
- [ ] **4.8.2 New Webhook** — `/new/webhook` → `POST /create` with `kind=webhook` (hidden), `name`, `url` (SSRF-checked), optional `secret`.
- [ ] **4.8.3 New Slack** — `kind=slack`, `name`, `url` (SSRF-checked).
- [ ] **4.8.4 New Email** — `kind=email`, `name`, `url` (Postmark, pre-filled), `secret` (server token), `from_address`.
- [ ] **4.8.5 Test** — `POST /{id}/test` sends a test event via the provider.
- [ ] **4.8.6 Delete** — `POST /{id}/delete` (data-confirm).
- [ ] **4.8.7 Field name** — confirm the kind field is `kind`, not `type`.

**Alerts & Digests — `/web/settings/alerts/`**
- [ ] **4.8.8 Create alert rule** — `POST /alerts/rules/create` (`project_id?`, `fingerprint?`, `threshold_count`, `window_secs`, `cooldown_secs`).
- [ ] **4.8.9 Delete alert rule** — `POST /alerts/rules/{id}/delete` (data-confirm).
- [ ] **4.8.10 Create digest** — `POST /alerts/digests/create` (`project_id?`, `interval_secs` = 86400/604800/3600).
- [ ] **4.8.11 Delete digest** — `POST /alerts/digests/{id}/delete` (data-confirm).

**Browser Defaults — `/web/settings/defaults/`**
- [ ] **4.8.12 Save** — `POST /defaults/` (`status`/`level`/`period`) sets a `stackpit_defaults` cookie (1-year, pipe-separated value).
- [ ] **4.8.13 Clear** — `POST /defaults/clear` deletes the cookie (Max-Age=0).
- [ ] **4.8.14 Effect** — after saving, §4.1/§4.3 list defaults reflect the saved values.

### §4.9 Per-project integrations

Route: `GET /web/projects/{project_id}/settings/integrations/`.

- [ ] **4.9.1 Activate** — `POST /activate` (`integration_id` select of available globals, notify checkboxes, `min_level`, `environment_filter`, optional `to_address`).
- [ ] **4.9.2 Update** — `POST /{id}/update` toggles `notify_new_issues`/`regressions`/`threshold`/`digests`, `min_level`, `environment_filter`.
- [ ] **4.9.3 Deactivate** — `POST /{id}/delete`.
- [ ] **4.9.4 End-to-end notify** — activate, set a min level + env filter, ingest a matching event, observe the notification fire (and a non-matching event suppressed).
- [ ] **4.9.5 Empty states** — no active integrations (link to globals) / none available ("Create one first").

### §4.10 Cross-project / global pages

- [ ] **4.10.1 Events firehose** — `/web/events/`: filter `query`/`level`/`item_type`/`project_id`; `POST /web/events/bulk` delete (`mode` selected/all_matching).
- [ ] **4.10.2 Defaults redirect** — `/web/events/` honours browser-defaults for `level`/`item_type`.
- [ ] **4.10.3 Releases** — `/web/releases/`: filter `query`/`project_id`/`period` (default `24h`); non-empty after the seed's release pass.

### §4.11 Static assets

Covered by curl in §2.4; in-browser confirm there are no 404s and no CSP console errors loading `bulk.js` / `confirm.js` / `stop-propagation.js`.

- [ ] **4.11.1** No `/web/_assets/*` 404 in the network panel on any admin page.
- [ ] **4.11.2** No CSP violation for inline-blocked scripts in the console.

---

## §5 — Public ingest

Ingest port only. Auth via `X-Sentry-Auth: Sentry sentry_key=<hex>` header or `?sentry_key=`.

### §5.1 Endpoints reachable

- [ ] **5.1.1 Envelope** — `POST /api/{id}/envelope` (and trailing-slash variant) accepts `application/x-sentry-envelope`.
- [ ] **5.1.2 Store** — `POST /api/{id}/store` accepts a single JSON event.
- [ ] **5.1.3 Security** — `POST /api/{id}/security` accepts a CSP/NEL report.
- [ ] **5.1.4 Minidump** — `POST /api/{id}/minidump` accepts multipart with `upload_file_minidump`.
- [ ] **5.1.5 Landing** — ingest `GET /` returns the "ingest port" info HTML.

### §5.2 Open-mode auth (`auth_service.rs::validate_project_key`)

Expected sequence is **401 / 200 / 401 / 401**:

- [ ] **5.2.1 Wrong key, existing project** → 401 (`X-Sentry-Error: unknown key for existing project`). **(was: pre-May-2026 checklists expect 200 — that's stale; first DSN wins.)**
- [ ] **5.2.2 First event, brand-new project** → 200; auto-provisions project + key.
- [ ] **5.2.3 Different key, now-existing project** → 401.
- [ ] **5.2.4 No auth** → 401 (`X-Sentry-Error: missing sentry key`).
- [ ] **5.2.5 Key/project mismatch** — valid key, wrong `project_id` in URL → 401 (`key/project mismatch`).
- [ ] **5.2.6 Max projects** — open mode at `filter.max_projects` → 403 (`max projects reached`).

### §5.3 Closed mode (set `[filter] mode = "closed"`, restart)

- [ ] **5.3.1** Any key not already in the DB → 401 (`project or key denied`); no auto-provision.

### §5.4 Archive gating

- [ ] **5.4.1 Archived → 403** — archive a project (§4.7.4), then ingest → 403 (`project is archived`).
- [ ] **5.4.2 Unarchive → 200** — unarchive (§4.7.5), ingest succeeds. **Note the 5-min auth cache (`AUTH_CACHE_TTL=300`):** confirm whether the settings handler calls `invalidate_project` — if not, ingest may keep 403-ing for up to 5 min after unarchive.

### §5.5 Filter / rate-limit responses

- [ ] **5.5.1 Per-project rate limit** → 429 with `X-Sentry-Rate-Limits` + `Retry-After`.
- [ ] **5.5.2 Blocked UA/IP** → 200 with a fake UUID (silent drop, no leakage).
- [ ] **5.5.3 All events filtered** → 200 with `X-Sentry-Discarded: <n>`.
- [ ] **5.5.4 Writer queue full** → 503 with `Retry-After: 60`.

---

## §6 — API surfaces

### §6.1 JSON API `/api/v1/*` (admin port, Bearer admin_token)

- [ ] **6.1.1 Projects** — `GET /api/v1/projects/` → 200.
- [ ] **6.1.2 Issues** — `GET /api/v1/projects/{id}/issues/` (params `status`/`level`/`query`/`limit`/`offset`) → 200.
- [ ] **6.1.3 Events** — `GET /api/v1/projects/{id}/events/` → 200; `GET /api/v1/events/{event_id}/` → 200/404.
- [ ] **6.1.4 Issue get/update** — `GET /api/v1/issues/{fp}/` → 200/404; `PUT` with `{"status":"resolved"}` → 200; `GET /api/v1/issues/{fp}/events/` and `/events/latest/`.
- [ ] **6.1.5 Alerts CRUD** — `GET /api/v1/alerts/rules` → 200; `POST` → 201 `{"id":…}`; `PUT/DELETE /api/v1/alerts/rules/{id}` → 204/404.
- [ ] **6.1.6 Digests CRUD** — `GET /api/v1/digests` → 200; `POST` → 201; `PUT/DELETE /api/v1/digests/{id}` → 204/404.
- [ ] **6.1.7 No auth** — every `/api/v1/*` without a valid bearer → 401 `{"error":"unauthorized"}`.
- [ ] **6.1.8 Error shapes** — 404 → `{"detail":…}`, 500 → `{"detail":"internal server error"}`.

### §6.2 Sentry-compat `/api/0/*` (INGEST port, Bearer `spk_` sourcemap key)

- [ ] **6.2.1 Project validate** — `GET /api/0/projects/{org}/{slug}/` → 200 JSON.
- [ ] **6.2.2 Create release** — `POST /api/0/organizations/{org}/releases/` and `/projects/{org}/{slug}/releases/` → 201.
- [ ] **6.2.3 Update release** — `PUT .../releases/{version}/` → 200.
- [ ] **6.2.4 Chunk upload** — `GET .../chunk-upload/` → 200 config; `POST` (multipart, ≤32 MB) → 200.
- [ ] **6.2.5 Assemble** — `POST .../artifactbundle/assemble/` → 200 `{state:"ok"}` or `{state:"not_found",missingChunks:[…]}`.
- [ ] **6.2.6 No/invalid bearer** → 401 `{"detail":"authentication required"|"invalid API key"}`.
- [ ] **6.2.7 Wrong project** → 403 `{"detail":"API key not valid for this project"}`.
- [ ] **6.2.8 Wrong port** — confirm `/api/0/*` on the **admin** port (`:3333`) is 404 (it lives on ingest).

---

## §7 — OIDC / SSO (feature/oidc)

Requires `[auth.oauth]` configured and `STACKPIT_MASTER_KEY` exported. See the skill's
"OAuth/SSO setup" section for the local Hydra (`~/git/forseti`) wiring.

### §7.1 Boot / config gating

- [ ] **7.1.1 Master key required** — OAuth enabled + `STACKPIT_MASTER_KEY` unset → startup hard-fails ("Server-side tokens must be encrypted at rest").
- [ ] **7.1.2 Malformed key** — `STACKPIT_MASTER_KEY` not 64 hex chars → startup hard-fails.
- [ ] **7.1.3 Discovery failure, required=false** → boots admin-token-only, SSO disabled, error logged.
- [ ] **7.1.4 Discovery failure, required=true** → startup hard-fails.
- [ ] **7.1.5 Issuer scheme** — `http://` issuer allowed only for loopback hosts (`localhost`/`127.0.0.1`/`[::1]`/`host.containers.internal`); any other `http://` issuer hard-fails (`config/url.rs::validate_issuer_url_scheme`).
- [ ] **7.1.6 Web audience** — empty `web_audience` without `allow_empty_web_audience=true` → hard-fail.

### §7.2 Login flow

- [ ] **7.2.1 SSO button** — `/web/login` shows "Sign in with SSO" (a link to `/web/auth/login`) above the break-glass token form when OAuth is live.
- [ ] **7.2.2 Init** — `GET /web/auth/login` sets `sp_login` cookie (HttpOnly, SameSite=Lax, Path=/web/auth/, Max-Age=600) and 302s to the IdP with PKCE S256 + state + nonce, scopes `openid email profile offline_access`.
- [ ] **7.2.3 Callback success** — `GET /web/auth/callback?code=&state=` verifies state + id_token + `email_verified`, upserts `users`, inserts an `oidc_grants` row, sets `sp_grant` (HttpOnly, SameSite=Strict, session) or `__Host-sp_grant`, clears `sp_login`, 302s to `/web/`.
- [ ] **7.2.4 DB rows** — `SELECT user_id, iss, sub, name FROM users` = 1 row; `SELECT user_id, iss, sub, datetime(access_exp,'unixepoch') FROM oidc_grants` = 1 row; `(iss, sub)` matches the IdP identity.
- [ ] **7.2.5 Grant encryption** — `access_token`/`refresh_token`/`id_token` columns are AES-256-GCM blobs (not plaintext JWTs).
- [ ] **7.2.6 Error redirects** — tampered state → `/web/login?error=state_mismatch`; expired `sp_login` → `error=session_expired`; the login page renders the friendly message, never the raw code.

### §7.3 Session / refresh / logout

- [ ] **7.3.1 Authenticated browse** — with a valid `sp_grant`, `/web/projects/` loads without the admin token.
- [ ] **7.3.2 Refresh-ahead** — when `access_exp - now ≤ 60s` and a refresh token exists, the next request rotates tokens transparently (`oidc/refresh.rs`); `oidc_grants` access token changes.
- [ ] **7.3.3 InvalidGrant** — IdP-rejected refresh forces re-login (grant deleted, redirect to `/web/login`) unless a concurrent refresh already rotated.
- [ ] **7.3.4 RP logout** — `POST /web/auth/logout` deletes the grant, clears `sp_grant`, redirects to the IdP `end_session_endpoint` with `id_token_hint` when available, else `/web/login?logout=local`.
- [ ] **7.3.5 CSRF on logout** — `POST /web/auth/logout` **is** CSRF-protected (only `/web/auth/login`, `/web/auth/callback`, `/web/auth/backchannel-logout` are exempt).
- [ ] **7.3.6 Per-grant CSRF** — under OIDC the CSRF token is random-per-grant (stored in `oidc_grants.csrf_token`), not the admin HMAC token.

### §7.4 Back-channel logout

- [ ] **7.4.1 Valid logout token** — `POST /web/auth/backchannel-logout` (form `logout_token=<jwt>`) → 200 (`Cache-Control: no-store`); writes an `oidc_revocations` marker (sid-scoped preferred) and deletes matching grants.
- [ ] **7.4.2 Replay** — re-posting the same `jti` → 400 (unique-constraint dedupe in `oidc_logout_jti`).
- [ ] **7.4.3 Malformed token** — missing `events` marker, present `nonce`/`exp`, stale `iat` (±60s), or wrong `aud` → 400.
- [ ] **7.4.4 Revocation effect** — after a back-channel logout, the revoked session's `sp_grant` no longer authenticates (gate fails closed).

### §7.5 No-auth loopback mode

- [ ] **7.5.1 Acknowledged** — no admin_token + no OAuth + loopback bind + `no_auth_loopback_acknowledged=true` → boots; all requests pass with `csrf_token=noauth`.
- [ ] **7.5.2 Refused** — same without the ack, or on a non-loopback bind → startup hard-fails.

---

## §8 — Security middleware

Middleware order (outer→inner): security_headers → rate_limit → web_auth → csrf → handlers.

### §8.1 Security headers (on `/web/projects/` and any admin response)

- [ ] **8.1.1 CSP** — `default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; img-src 'self' data:; frame-ancestors 'none'; object-src 'none'; base-uri 'self'; form-action 'self'`. **`script-src` must NOT carry `unsafe-inline`** (stale-binary tell). `style-src` currently *does* carry `unsafe-inline` (pending inline-style extraction — expected, not a regression).
- [ ] **8.1.2 X-Content-Type-Options** — `nosniff`.
- [ ] **8.1.3 X-Frame-Options** — `DENY` (plus `frame-ancestors 'none'`).
- [ ] **8.1.4 Referrer-Policy** — `strict-origin-when-cross-origin`.
- [ ] **8.1.5 Cache-Control** — `no-store, private` on `/web/*`.

### §8.2 CSRF scope

- [ ] **8.2.1 Web POSTs** — all mutating `/web/*` forms require `csrf_token` (no/wrong → 403) except the login POST.
- [ ] **8.2.2 Cookie-authed API** — `/api/*` POST/PUT/DELETE/PATCH with a `Cookie` header but no `Authorization: Bearer` is CSRF-checked.
- [ ] **8.2.3 Bearer-authed API exempt** — `/api/v1/*` with `Authorization: Bearer` is not CSRF-checked.
- [ ] **8.2.4 Deterministic admin token** — re-login with the same admin_token yields the same CSRF token.

### §8.3 Rate limits

- [ ] **8.3.1 Login limit** — `POST /web/login` 10/min/IP: attempts 1–10 → 401 (wrong token), 11th → 429 with `Retry-After: 60`. (Sleep ≥65s first to clear the bucket.)
- [ ] **8.3.2 Admin limit** — other admin endpoints 120/min/IP → 429 on exceed.
- [ ] **8.3.3 Window** — fixed 60s bucket; the limit resets after the window.

---

## §9 — Organizations, roles & multi-tenant isolation

Two identity paths exercise this: the **admin_token** superuser (§9.7) and real **OIDC users** (§9.1–§9.6, §9.9, needs the OIDC stack from §7). Org *structure* is created by driving the app, so the create/invite/rename flows are themselves test cases. Fill a project with volume by grabbing its DSN and running `python3 scripts/generate-fake-data.py --dsn '<project-dsn>' --count 300` (routes events into that exact project over the real ingest path; no `--dsn` = the default 100-project seed).

Two distinct OIDC users are needed for §9.3/§9.5 (an owner and an invited member). A superuser cannot create orgs (§9.1.1) — org creation is an OIDC-user action.

### §9.1 Org creation & switcher (OIDC user)

- [ ] **9.1.1 Create org** — `/web/organizations` → create a native org. Slug auto-derived from the name (slugified), you become owner. The admin_token superuser gets **403** here (org creation is real-users-only).
- [ ] **9.1.2 Switcher persists + re-filters** — switch active org; `sp_active_org` cookie updates. The project list, issue list, event/release firehoses, alert rules, and digest schedules all re-filter to the active org.
- [ ] **9.1.3 Switch to non-member org → 403** — `POST /web/organizations/switch` to an org you're not a member of is rejected.
- [ ] **9.1.4 Create-in-active-org** — `+ New project` while an org is active assigns the new project to that org (not the System org). Verify `projects.org_id`.
- [ ] **9.1.5 DSN seeding** — copy the new project's DSN (settings/keys), run `generate-fake-data.py --dsn …`; events + issues land in that project and only in its org.

### §9.2 Personal org

- [ ] **9.2.1 Auto-create + neutral slug** — first OIDC login auto-creates a personal org (`is_personal=1`, you're owner). Slug is a neutral `personal-<hex>` token — **not** `user-<id>` (no sequential id / PII leak).
- [ ] **9.2.2 Idempotent** — re-login does not create a second personal org and does not change its slug (a §9.4 rename survives re-login).

### §9.3 Members & invites

- [ ] **9.3.1 Members page** — `/web/organizations/{id}/members` lists members and pending invites. A **member** sees a read-only roster (no slug/invite/danger-zone/remove controls); an **owner** additionally sees the management forms. A **non-member** gets 404 (existence hidden, not 403). (Verified 2026-07-01: member view is roster-only; management controls owner-gated.)
- [ ] **9.3.2 Create invite** — role + optional email + optional expiry. **A blank expiry must default to 7 days, not 400** (regression: empty `ttl_secs` must deserialize to the default). An explicit expiry is honored. The invite URL is shown once.
- [ ] **9.3.3 Accept + single-use** — the invite preview shows org + role; accept → the invitee joins with the invite's role; `accepted_by`/`accepted_at` set; the link is single-use (second accept rejected).
- [ ] **9.3.4 Revoke pending invite** — `POST /web/organizations/{id}/invites/{invite_id}/revoke` removes a pending invite; the once-shown link no longer accepts; a member (non-owner) attempting revoke → 404/403 with no effect.
- [ ] **9.3.5 Change member role** — `POST /web/organizations/{id}/members/{user_id}/role` toggles a member owner↔member; the target's access changes on next request. IDOR: authz is on the **path** org, never the actor's active org.
- [ ] **9.3.6 Remove member** — `POST /web/organizations/{id}/members/{user_id}/remove` deletes the membership row; the removed user loses access to the org's projects.
- [ ] **9.3.7 Last-owner guard** — the sole owner cannot be removed or downgraded: the guarded write affects 0 rows and returns FORBIDDEN, membership unchanged (verify the owner row survives).

### §9.4 Slug rename

- [ ] **9.4.1 Owner rename** — the members page's slug card renames the org slug (slugified, globally unique). Redirects back on success.
- [ ] **9.4.2 Conflict → 409** — renaming to an existing slug returns 409 and leaves the slug unchanged (no silent suffix for a user-chosen value).
- [ ] **9.4.3 Scope** — an owner can rename their own **personal** org; the **system** org and **Forseti-backed** orgs are rejected both in the UI (hidden) and at the query layer (bail).

### §9.5 Role gates (member vs owner)

- [ ] **9.5.1 Member reads → 200** — a member of the org can view the project, issue list/detail, event detail, and the settings *page*.
- [ ] **9.5.2 Member mutations → 403** — every owner-gated write is blocked for a member, with **no side effect**: project rename/archive/delete, key create/delete, repo add/delete, filter writes, integration activate/update/delete, alert-rule & digest create/update/delete, bulk resolve/ignore/delete, issue status update, create invite, slug rename.
- [ ] **9.5.3 Owner allowed** — the same actions as an owner succeed (2xx / 303).

### §9.6 Cross-org isolation (the surfaces `c185b28` closed)

- [ ] **9.6.1 Cross-org access → 404** — a scoped user opening a project / issue / event that belongs to another org (URL guessing) gets **404**, not 200 and not 403.
- [ ] **9.6.2 Fingerprint / event lookup** — cross-org issue-by-fingerprint and event lookups are denied (`project_of_fingerprint` / `project_of_event` + `require_project_scope`).
- [ ] **9.6.3 Firehoses scoped** — `/web/events/` and `/web/releases/` show only the active org's rows (no leak of other orgs' events).
- [ ] **9.6.4 Jobs scoped** — issue list, digest generation, and threshold-alert evaluation only include the active org's rows.
- [ ] **9.6.5 Integration linking** — an owner cannot link another org's integration in project settings.

### §9.7 Superuser (admin_token)

- [ ] **9.7.1 Unassigned view** — `/web/admin/unassigned` lists System-org projects.
- [ ] **9.7.2 Reassignment** — `POST /web/admin/projects/{id}/assign` moves a project to an org (303); a no-CSRF POST → 403; verify `projects.org_id` changed.
- [ ] **9.7.3 Switch to any org** — the superuser may switch active org to any org, including System and other users' personal orgs (contrast §9.1.3).
- [ ] **9.7.4 Direct-access bypass** — the superuser can open any project / issue directly regardless of active org (per-project scope is bypassed).
- [ ] **9.7.5 Lists still active-org-scoped** — project *lists* (web and `/api/v1/projects/`) reflect the active org even for the superuser; the bypass applies to direct access, not enumeration.

### §9.8 OIDC org provisioning & reconciliation (IdP emits an `orgs` claim)

Cross-ref §7.2. Requires an IdP that emits org claims (the local Forseti dev stack may not by default; the unit tests in `src/orgs/reconcile.rs` cover the logic).

- [ ] **9.8.1 Provisioning** — login with an `orgs` claim (role owner) provisions a Forseti org (`ext_iss`/`ext_org_id` set) and adds membership; it appears in the switcher.
- [ ] **9.8.2 Reconciliation** — re-login with changed claims adds/upgrades/removes memberships accordingly.
- [ ] **9.8.3 Safety** — the last-owner guard blocks removing/downgrading the sole owner; `role_sync` promotes/demotes per the claim when enabled.

### §9.9 Org deletion (danger zone)

Route: `POST /web/organizations/{org_id}/delete` (handler `orgs::delete_org`, `src/html/orgs.rs`). The danger zone renders on the members page for the owner (and for the superuser on Native/Forseti orgs). Typed-slug confirmation via the `confirm_slug` field. Only **native** and **Forseti-backed** orgs are deletable; **system** and **personal** orgs are refused.

- [ ] **9.9.1 Counts accurate** — the danger-zone copy shows project + member counts (`project_count`/`member_count`); cross-check against `projects`/`organization_members` for the org. Deletion also cascades invites, integrations, alert_rules, digest_schedules (per `DeleteOrgCounts`).
- [ ] **9.9.2 Wrong slug → 400** — POST with `confirm_slug` ≠ the org slug → 400 ("Type the organization slug to confirm deletion."); org survives.
- [ ] **9.9.3 Missing CSRF → 403** — POST delete with no `csrf_token` → 403; org survives (CSRF layer not exempted).
- [ ] **9.9.4 Non-owner → 404** — a member (or non-member) POSTing delete gets **404** (existence hidden), not 403; org survives.
- [ ] **9.9.5 System + personal refused** — the danger zone is hidden for system and personal orgs, and a forced POST bails at the query layer (`NotDeletable`); neither is destroyed.
- [ ] **9.9.6 Success cascade** — correct `confirm_slug` → 303 to `/web/organizations`; the org row and all org-scoped rows are gone (projects, members, invites, integrations, alert_rules, digest_schedules) plus the projects' events/issues. The `delete_org_covers_all_org_scoped_tables` guard test backs the table set.
- [ ] **9.9.7 Active-org cookie cleared** — if the deleted org was the active org, `sp_active_org` is cleared (not repacked); the next request falls back to a valid org.
- [ ] **9.9.8 Audit log** — a `tracing::warn` audit line records actor, `org_id`, kind, and the cascade counts.
