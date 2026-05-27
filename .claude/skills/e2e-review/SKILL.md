---
name: e2e-review
description: End-to-end review of the stackpit stack. Prompts the operator to choose between a fast automated tier (cargo test + curl-driven API probes) and an interactive Chrome-MCP walkthrough of docs/checklist-testing.md. The interactive path wipes state, seeds 100 named projects with 5k+ events via scripts/generate-fake-data.py, then drives Chrome per a menu of surfaces, reporting what worked, what broke, and which DB rows landed. Use when the user says "run the e2e review", "test stackpit end to end", "regression-test the admin UI", "verify the stack after my changes", or similar.
---

# e2e-review

The canonical, numbered list of test cases lives in **`docs/checklist-testing.md`** (§0–§8). This skill is the *driver*: it picks a tier, brings up the stack, and walks that checklist. When a step here cites a `§`, it refers to that file. Keep the two in sync — if you add a surface, add its `§` to `docs/checklist-testing.md` first.

Goal: take stackpit from "fresh clone" to "every flow verified" — admin UI, public ingest, both auth paths, security middleware, and the two API surfaces. Three tiers, operator picks:

1. **Rust tests** — `cargo test --no-default-features --features sqlite` covers the unit + integration suite (~1 min after the first build). Best for "did my data-layer change break anything".
2. **Curl-driven API probes** — seeds 5k+ events via `scripts/generate-fake-data.py`, then drives `/web/login`, CSRF, ingest, JSON API, and security-header checks via `curl`. ~30s once the server is up. Catches the "endpoint moved or status code drifted" class.
3. **Interactive Chrome-MCP walkthrough** — drives a real browser via the `claude-in-chrome` MCP through `docs/checklist-testing.md` (§§3–8). Slow (~30 min), exhaustive, catches what (1) and (2) miss. Wipes state. Best for new features, post-release verification, or when the operator wants eyes on the UX.

Tiers (1) and (2) are fire-and-forget; tier (3) is destructive and chatty — every visible step prompts the operator and waits for a "looks right?" sign-off.

## Mode selection (ask first)

Before doing anything, prompt the operator:

> "How would you like to verify the stack?
> (a) **Automated suite** — fast (~2 min total), runs `cargo test` and the curl-driven API probe pass. No wipe.
> (b) **Interactive browser walkthrough** — slow (~30 min), exhaustive. Wipes state, seeds fake data, drives `docs/checklist-testing.md` via the claude-in-chrome MCP.
> (c) **Both** — run (a) first to catch the easy ones, then (b) for anything not covered."

Default suggestion when in doubt: (c). The cargo build dominates wall-clock anyway, so the marginal cost of running both is small.

## Prereqs (ALL tiers)

* Repo checked out locally, with a **freshly rebuilt release binary**. **The single biggest time-waster in this skill is testing a stale binary** — symptoms include `/web/_assets/*.js` 404, `'unsafe-inline'` in CSP, double-submit CSRF cookies, missing/changed routes. Always rebuild before testing:
  ```bash
  guix shell -m manifest.scm -- cargo build --release
  ```
  Build takes ~3-4 min cold, seconds on incremental.
* `stackpit.toml` at repo root with at minimum `[server]` + `[storage]` + `[filter] mode = "open"`. If missing, run `./target/release/stackpit init` (regenerates a fresh `admin_token` automatically).
* Ports `:3333` (admin) + `:3334` (ingest) free. The default `stackpit init` writes `:3000/:3001`; this skill uses `:3333/:3334` so it won't clash with anything already bound to the defaults. Rewrite `bind`/`ingest_bind` in `stackpit.toml` if you see "Address already in use".
* For tier (3): `claude-in-chrome` MCP available; Chrome reachable. Start with a fresh tab via `tabs_create_mcp` — never reuse tabs from prior MCP sessions.
* `python3` for the fake-data script (no extra deps; `urllib3` is optional, falls back to `urllib`).

## Tier 1 — Rust tests

> There is now a real integration suite (`tests/integration/`) that codifies
> the tier-2 auth/CSRF/ingest/security probes below as Rust tests. Run it with
> `guix shell make sqlite -- make test-integration` after `make serve-bg && make seed`
> (serve-bg wipes the DB; you cannot reseed a running server). A Playwright
> smoke pass (`guix shell make -- make e2e`) covers the login → project-create →
> issue-list → rename path in a real browser. `make test` remains unit-only.

```bash
cd "$(git rev-parse --show-toplevel)"
guix shell -m manifest.scm -- cargo test --no-default-features --features sqlite
```

Acceptance: all tests pass. Postgres tier is optional and needs a live DB (`DATABASE_URL=postgres://... cargo test --no-default-features --features postgres -- --test-threads=1`). Skip unless explicitly asked.

Report pass/fail counts and any failed test name. Don't try to fix in this skill — hand the test name back to the operator.

## Tier 2 — Curl-driven API probes

### 2.1 Bring up the stack

```bash
cd "$(git rev-parse --show-toplevel)"
# Kill any stale stackpit (do NOT use `pkill -f 'stackpit serve'` -- it
# matches your own bash session and kills the parent shell, exit code 144).
PID=$(ss -ltnp 2>/dev/null | awk '/:3333 /{print}' | grep -oE 'pid=[0-9]+' | head -1 | sed 's/pid=//')
[ -n "$PID" ] && kill "$PID"
sleep 1
rm -f stackpit.db stackpit.db-wal stackpit.db-shm
nohup ./target/release/stackpit serve > /tmp/stackpit.log 2>&1 & disown
sleep 2
cat /tmp/stackpit.log | tail -5
```

Acceptance: log shows `ingestion listening on 127.0.0.1:3334` + `admin listening on 127.0.0.1:3333`. `STACKPIT_MASTER_KEY` warning is expected if no encryption key is exported.

### 2.2 Seed fake data

```bash
python3 scripts/generate-fake-data.py --count 500 --quiet
```

What this does (auto-discovers `admin_token` + admin URL from `stackpit.toml`):
1. Ingests ~5k events across 100 projects (events, transactions, sessions, CSP reports, check-ins, attachments, logs, spans, metrics, profiles, replays, user/client reports).
2. Pauses 65s mid-way to stay under the admin rate limit.
3. Post-ingest setup: renames all 100 auto-provisioned projects to readable slugs (`checkout-engine`, `tax-worker`, etc.), mints a `spk_…` sourcemap key per project, seeds 2-4 releases per project via the Sentry-compat API on `:3334`.

Acceptance summary:
```
Done: ~5100 ok, 0 failed
setup: named 100/100 projects, 100 sourcemap keys, ~300 releases
```

Verify the DB matches:
```bash
sqlite3 stackpit.db "SELECT 'events',COUNT(*) FROM events;
SELECT 'issues',COUNT(*) FROM issues;
SELECT 'logs',COUNT(*) FROM logs;
SELECT 'spans',COUNT(*) FROM spans;
SELECT 'metrics',COUNT(*) FROM metrics;
SELECT 'releases',COUNT(*) FROM releases;
SELECT 'named_projects',COUNT(*) FROM projects WHERE name IS NOT NULL AND name<>'';"
```

All counts should be non-zero. `named_projects` = 100. If `logs`/`spans`/`metrics` = 0, the script regressed.

### 2.3 Auth + CSRF + headers

```bash
TOK=$(grep '^admin_token' stackpit.toml | sed 's/.*"\(.*\)".*/\1/')
C=$(mktemp)

# Login -- expect 303 to /web/projects/, stackpit_token cookie HttpOnly+SameSite=Strict,
# value == SHA-256(token).
curl -sS -i -X POST -d "token=$TOK" http://127.0.0.1:3333/web/login -c "$C" -o /dev/null
echo "stackpit_token cookie value (should = SHA-256 of admin_token):"
grep stackpit_token "$C"
printf '%s' "$TOK" | sha256sum

# Wrong token -- expect 401, "Invalid token" body, login form re-rendered
curl -sS -i -X POST -d 'token=wrong' http://127.0.0.1:3333/web/login | head -5

# Synchronizer-token CSRF -- value rendered into <input>, deterministic per admin_token.
CSRF=$(curl -sS -b "$C" http://127.0.0.1:3333/web/projects/1/settings/ \
  | grep -oE 'csrf_token" value="[a-f0-9]+' | head -1 | sed 's/.*value="//')
echo "csrf=$CSRF"

# CSRF: POST with no token -> 403
curl -sS -X POST -d 'name=x' http://127.0.0.1:3333/web/projects/1/settings/name -b "$C" -o /dev/null -w "no csrf  HTTP %{http_code}\n"
# CSRF: POST with wrong token -> 403
curl -sS -X POST -d "name=x&csrf_token=00000000000000000000000000000000" http://127.0.0.1:3333/web/projects/1/settings/name -b "$C" -o /dev/null -w "wrong csrf  HTTP %{http_code}\n"
# CSRF: POST with correct token -> 200
curl -sS -X POST -d "name=RenamedByProbe&csrf_token=$CSRF" http://127.0.0.1:3333/web/projects/1/settings/name -b "$C" -o /dev/null -w "good csrf  HTTP %{http_code}\n"

# Security headers on /web/projects/
curl -sS -I -b "$C" http://127.0.0.1:3333/web/projects/ | grep -iE 'cache-control|content-security|x-frame|x-content|referrer'
rm -f "$C"
```

Acceptance:
- Login 303, cookie value = SHA-256 of `admin_token`, HttpOnly, SameSite=Strict ✓
- Wrong token 401 ✓
- CSRF no/wrong → 403, correct → 200 ✓
- CSP includes `script-src 'self'` (no `'unsafe-inline'`), plus `object-src 'none'`, `base-uri 'self'`, `form-action 'self'` ✓
- `Cache-Control: no-store, private` on `/web/*` ✓

### 2.4 Ingest auth (open mode)

```bash
# Wrong key on EXISTING project -> 401 (was 200 before the open-mode tightening)
curl -sS -o /dev/null -w "wrong key on existing project  HTTP %{http_code}\n" \
  -X POST -H 'Content-Type: application/x-sentry-envelope' \
  -H 'X-Sentry-Auth: Sentry sentry_key=deadbeefdeadbeefdeadbeefdeadbeef' \
  --data-binary $'{"event_id":"a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"}\n{"type":"event","length":2}\n{}' \
  http://127.0.0.1:3334/api/1/envelope

# First event for a brand-new project -> auto-provisions both project AND key (200)
curl -sS -o /dev/null -w "first event on new project 999  HTTP %{http_code}\n" \
  -X POST -H 'Content-Type: application/x-sentry-envelope' \
  -H 'X-Sentry-Auth: Sentry sentry_key=11111111111111111111111111111111' \
  --data-binary $'{"event_id":"b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"}\n{"type":"event","length":2}\n{}' \
  http://127.0.0.1:3334/api/999/envelope

# Second event on now-existing project 999 with DIFFERENT key -> 401 (first DSN wins)
curl -sS -o /dev/null -w "different key on existing project 999  HTTP %{http_code}\n" \
  -X POST -H 'Content-Type: application/x-sentry-envelope' \
  -H 'X-Sentry-Auth: Sentry sentry_key=22222222222222222222222222222222' \
  --data-binary $'{"event_id":"c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3"}\n{"type":"event","length":2}\n{}' \
  http://127.0.0.1:3334/api/999/envelope

# No auth at all -> 401 ("missing sentry key")
curl -sS -o /dev/null -w "no auth  HTTP %{http_code}\n" \
  -X POST -H 'Content-Type: application/x-sentry-envelope' \
  --data-binary '{}' http://127.0.0.1:3334/api/1/envelope
```

Acceptance: `401 / 200 / 401 / 401` in that order. Anything else means the open-mode auth tightening (see `src/auth_service.rs::validate_project_key`) regressed.

### 2.5 JSON API + Sentry-compat API

```bash
TOK=$(grep '^admin_token' stackpit.toml | sed 's/.*"\(.*\)".*/\1/')

# /api/v1/* -- admin port, admin_token Bearer
for url in /api/v1/projects/ /api/v1/projects/1/issues/ /api/v1/projects/1/events/ /api/v1/alerts/rules /api/v1/digests; do
  curl -sS -H "Authorization: Bearer $TOK" -H 'Accept: application/json' -o /dev/null -w "$url  HTTP %{http_code}\n" "http://127.0.0.1:3333$url"
done
# Wrong bearer -> 401
curl -sS -H "Authorization: Bearer wrong" -H 'Accept: application/json' -o /dev/null -w "wrong bearer  HTTP %{http_code}\n" http://127.0.0.1:3333/api/v1/projects/

# /api/0/* -- INGEST port (yes really), needs an spk_ key. Pick any from the DB.
SPK=$(sqlite3 stackpit.db "SELECT 'spk_' || (SELECT prefix FROM api_keys WHERE scope='sourcemap' LIMIT 1);" 2>/dev/null)
# (the prefix isn't the full key -- if you don't have one saved from setup, mint a new one via /web/projects/{id}/settings/sourcemaps/generate)
```

Acceptance: all `/api/v1/*` 200 with token, 401 without.

### 2.6 Static assets + admin /health

```bash
for p in /health /web/_assets/style.css /web/_assets/icon.svg /web/_assets/bulk.js /web/_assets/confirm.js /web/_assets/stop-propagation.js; do
  curl -sS -o /dev/null -w "$p  HTTP %{http_code}  type=%{content_type}\n" "http://127.0.0.1:3333$p"
done
curl -sS http://127.0.0.1:3334/health   # ingest health is a JSON blob with counters
```

Acceptance: every line is 200, JS files have type `application/javascript`. Admin `/health` returns `ok`; ingest `/health` returns `{"events":{...},"status":"ok","writer":{...}}`.

### 2.7 Rate limits

```bash
# LOGIN_RATE_LIMIT = 10 per minute per IP. 10 succeed (all 401 because wrong token), 11th -> 429.
sleep 65  # let any prior bucket clear
for i in $(seq 1 11); do
  S=$(curl -sS -o /dev/null -w "%{http_code}" -X POST -d 'token=x' http://127.0.0.1:3333/web/login)
  echo "attempt $i  HTTP $S"
done
```

Acceptance: attempts 1-10 → 401, attempt 11 → 429.

If that's all green, Tier 2 is done. Hand the operator a one-line summary and stop, unless they also asked for Tier 3.

## Tier 3 — Interactive Chrome-MCP walkthrough

Drives a real browser through `docs/checklist-testing.md` §§3–8. The §0.5 UX rubric (next-step obvious, error messages actionable, success feedback, empty states useful, keyboard-only, screen-reader sanity, narrow viewport, destructive POSTs guarded, copy reads naturally, console clean) applies to every screen.

### 3.1 Confirm a wipe (if not already done in Tier 2)

> "About to wipe `stackpit.db` (events, issues, projects, OIDC grants, integrations, alerts — everything). Reseed via fake-data after? Continue?"

On approval, run the bring-up from §2.1 and the seed from §2.2.

### 3.2 Open a fresh browser tab

```text
tabs_context_mcp(createIfEmpty=true)
tabs_create_mcp()   # always a fresh tab, never reuse old ones
navigate to http://127.0.0.1:3333/web/login
```

Login: the admin_token is in `stackpit.toml`. Submit. Expect 303 → `/web/projects/`.

### 3.3 Present the menu

Ask which surface to verify next. **Skip items already covered by Tier 2** when the operator chose mode (c).

Suggested order:

1. **§4.1 Home / project list** — `/` and `/web/` both 308 to `/web/projects/`. 100 named projects listed. Filter form (`query`, `period`), sort headers (`Project | Issues | Events | First Seen | Last Seen`), per-period aggregate counts. Empty state copy when no projects.
2. **§4.2 Create project** — `/web/projects/new` with name + platform dropdown. Submit → `project_created.html` shows DSN. **The DSN host now comes from `external_ingest_url` → `external_url` → `http://{ingest_bind}` in that order**; confirm the displayed DSN points at the right port for the current config.
3. **§4.3 Issue list** — click a project. Tab strip hides zero-count tabs. Event histogram SVG renders. Filter form: `query`, `status`, `level`, `release`, `period`. Bulk bar appears when a row is ticked.
4. **§4.4 Issue detail** — click an issue. `<h1>` = error type. Sub-tabs Details/All Events. Exception & Stacktrace accordion open by default, `in_app` frames coloured. Tag pills link to filtered issue list. Sidebar Meta grid (Fingerprint, Level, Status, Events, Users, First/Last Seen + release). Status form round-trips, sparkline renders, tag facets render.
5. **§4.5 Event detail** — same accordion set, event-nav strip, full event payload sidebar.
6. **§4.6 Per-project tabs** — Transactions, Spans, Traces, Logs, Metrics, Monitors, Profiles, Replays, Health, User Reports, Client Reports. The seeded data exercises all of these for at least some projects (fake-data now emits all 5 of the previously-missing types).
7. **§4.7 Project settings** — `/settings/` (rename + repos + archive/unarchive + delete), `/settings/keys/` (DSN + add/delete keys), `/settings/sourcemaps/` (mint `spk_`), `/settings/filters/` (all 8 sub-sections round-trip). **Archive → ingest 403; unarchive → ingest 200.**
8. **§4.8 Global settings** — `/web/settings/integrations/` (add Webhook/Slack/Email; field is `kind`, not `type`), `/web/settings/alerts/` (rule fields: `threshold_count`, `window_secs`, `cooldown_secs`), `/web/settings/defaults/` (cookie `sp_defaults` with pipe-separated value).
9. **§4.9 Per-project integrations** — activate a global integration, set min level + env filter, trigger an event matching the rule, watch for the notification.
10. **§4.10 Cross-project / global pages** — `/web/events/` firehose, `/web/releases/` (now non-empty thanks to the fake-data setup pass).
11. **§4.11 Static assets** — covered in §2.6.
12. **§8 Security middleware** — headers + rate limits covered in §2.3 / §2.7.

For each picked step:
* Drive the browser via MCP (`navigate`, `find`, `form_input`, `javascript_tool`, `read_page`).
* Verify the DOM matches expectations (use `javascript_tool` to extract specific fields rather than dumping huge `read_page` output).
* Check the DB for any row that should have landed:
  ```bash
  sqlite3 stackpit.db "SELECT project_id, name, status FROM projects WHERE project_id = N;"
  sqlite3 stackpit.db "SELECT fingerprint, level, status, event_count FROM issues WHERE fingerprint = '...';"
  ```
* Apply the §0.5 UX rubric — note any gap as a finding, don't fix in this skill.
* Ask the operator "looks right?" — wait for explicit approval before moving on.

If something looks broken: name `file:line` where investigation should start and **STOP**. Don't try to fix anything in this skill — fixes are a separate task.

## Reporting

Hand the operator a structured report per the `docs/checklist-testing.md` template:

* What worked (one line per section)
* What broke (`file:line` of the suspect handler)
* DB rows that did / didn't land
* §0.5 UX-rubric findings (empty states, error copy, keyboard path, narrow viewport, etc.)
* Any side-findings (copy issues, missing pretty-fields, console errors, security-header regressions)

Don't auto-fix anything in a single review pass — record findings, hand back to the operator.

## OAuth/SSO setup (Hydra from `~/git/forseti`)

If `[auth.oauth]` is configured in `stackpit.toml`, you're testing the SSO path on top of the local forseti stack (Hydra + Kratos + forseti at `:4444 / :4445 / :4433 / :4434 / :3000`). Three pieces have to line up:

1. **Hydra client registered with `client_secret_basic`** — stackpit's `openidconnect` crate sends credentials via `Authorization: Basic`. Hydra's REST default on POST `/admin/clients` is `client_secret_post`; using that gives `invalid_client: Client authentication failed … OAuth 2.0 Client supports 'client_secret_post', but method 'client_secret_basic' was requested`. Register with:
   ```bash
   curl -sS -X POST http://127.0.0.1:4445/admin/clients \
     -H 'Content-Type: application/json' \
     -d '{
       "client_name": "stackpit",
       "grant_types": ["authorization_code", "refresh_token"],
       "response_types": ["code"],
       "scope": "openid email profile offline_access",
       "token_endpoint_auth_method": "client_secret_basic",
       "redirect_uris": ["http://127.0.0.1:3333/web/auth/callback"],
       "post_logout_redirect_uris": ["http://127.0.0.1:3333/web/login"]
     }'
   ```
   Capture `client_id` + `client_secret` from the response — they're write-once visible.

2. **`stackpit.toml` `[auth.oauth]`** uses Hydra's *issuer string*, not the bind address:
   ```toml
   [auth.oauth]
   issuer_url               = "http://host.containers.internal:4444"
   client_id                = "<from step 1>"
   client_secret            = "<from step 1>"
   redirect_uri             = "http://127.0.0.1:3333/web/auth/callback"
   post_logout_redirect_uri = "http://127.0.0.1:3333/web/login"
   required                 = false
   allow_empty_web_audience = true
   ```
   The issuer must be the literal string Hydra advertises in `/.well-known/openid-configuration` — the `openidconnect` crate cross-checks it.

3. **Boot env:**
   ```bash
   STACKPIT_MASTER_KEY=$(openssl rand -hex 32) ./target/release/stackpit serve
   ```
   `STACKPIT_MASTER_KEY` is mandatory when OAuth is on (the grant vault encrypts at rest); startup bails without it.

After boot, `/web/login` shows a **Sign in with SSO** button. If you already have a Kratos session (forseti Dashboard tab open), Hydra skips the login screen and goes straight to consent. Click Allow → land on `/web/projects/`. Verify:

```bash
sqlite3 stackpit.db "SELECT user_id, iss, sub, COALESCE(name,'(null)') FROM users;
SELECT user_id, iss, sub, datetime(access_exp,'unixepoch') FROM oidc_grants;"
```

— one row each, `(iss, sub)` matches the Kratos identity UUID.

**Notes on the local stack:**
* Hydra's `end_session_endpoint` is plain-http on `host.containers.internal:4444`. Stackpit drops it on discovery — RP-initiated logout signs out of Stackpit only, not the IdP. Set `allow_local_only_logout = true` to silence the boot warning.
* Hydra didn't issue a refresh token on the first round (no `prompt=consent`); access tokens expire in 5 min. Re-clicking SSO mints a new one without re-prompting (Kratos session still active).
* The `host.containers.internal` issuer string is a /etc/hosts alias for 127.0.0.1 on this host — stackpit's `validate_issuer_url_scheme` now includes it in the loopback whitelist alongside `localhost / 127.0.0.1 / [::1]`. If you flip to `localhost:4444`, Hydra's discovery doc still claims `issuer: http://host.containers.internal:4444` and the openidconnect crate rejects the mismatch.

## Known traps (avoid re-discovering)

* **Stale binary trip-wires.** Symptoms include `'unsafe-inline'` in CSP, `/web/_assets/bulk.js` 404, double-submit CSRF cookies that JS injects into forms, `/health` 404 on admin port. **All of these resolve after `cargo build --release`.** If you see any of them, stop testing and rebuild before recording findings.
* **Migration checksum mismatch.** Symptom: `Error: migration 7 was previously applied but has been modified`. Recovery: `rm -f stackpit.db*` and re-seed. Happens when a migration file is edited in-place between binary builds.
* **`pkill -f 'stackpit serve'` kills your own shell.** The bash command line that ran `nohup ./target/release/stackpit serve …` also matches the pattern. Exit code 144. Use this instead:
  ```bash
  PID=$(ss -ltnp 2>/dev/null | awk '/:3333 /{print}' | grep -oE 'pid=[0-9]+' | head -1 | sed 's/pid=//')
  [ -n "$PID" ] && kill "$PID"
  ```
* **Default ports may already be bound.** `stackpit init` writes `:3000/:3001`; if another service holds them, rewrite `bind`/`ingest_bind` (this skill uses `:3333/:3334`). Boot error otherwise: `Address already in use (os error 98)`.
* **Sentry-compat `/api/0/*` lives on the INGEST port (`:3334`), not the admin port (`:3333`).** Hitting `:3333/api/0/...` returns 404 and looks like a broken endpoint — it isn't, the route is on the other listener. JSON `/api/v1/*` and `/web/*` are the admin port.
* **Open-mode auth: first DSN wins.** Sending an unknown key to an existing project now returns 401 (was 200, auto-provisioned a key). If a checklist item from before May 2026 expects 200, the checklist is stale, not the code. See `src/auth_service.rs::validate_project_key`.
* **DSN host source.** Order of precedence: `external_ingest_url` → `external_url` → `http://{ingest_bind}`. Behind a single reverse proxy that fronts both listeners on one origin, leave `external_ingest_url` unset. Split-port local dev: set both, otherwise the DSN points at `:3333` (admin) and SDKs 404.
* **CSRF token is deterministic per admin_token.** Synchronizer-token pattern, HMAC-SHA256, rendered as `<input type="hidden" name="csrf_token" value="…">` in every POST form. **There is no CSRF cookie** post-rebuild; if you see one, you're on a stale binary. Re-login with the same admin_token returns the same CSRF token — useful for scripts.
* **Admin rate limit during scripted setup.** `LOGIN_RATE_LIMIT=10`, `ADMIN_RATE_LIMIT=120` per IP per minute. The fake-data setup pass chunks renames + sourcemap-key minting at 55 projects per burst with a 65s sleep — don't try to "speed it up" by removing the sleep; you'll get 429s mid-loop and only half the projects will be named.
* **Chrome safety filter blocks base32/hex-looking strings** from `read_page` / `javascript_tool` output (shows `[BLOCKED: Cookie/query string data]`). Workaround: extract specific fields via `javascript_tool` with explicit property access (e.g. `document.querySelector('form').method`) instead of dumping `outerHTML`. For cookies, use `Cookie:` headers in `curl` rather than asking the browser to read `document.cookie`.
* **Browser tabs persist across MCP sessions.** Always `tabs_create_mcp()` at the start of a Tier-3 run — reusing an old tab carries stale cookies, scroll position, and login state that confuses tests.
* **`--quiet` on the fake-data script is silent on non-tty only.** When piped to `tee` or redirected, it now drops the per-event progress spam (was ~135KB of `progress: N/M` on one un-terminated line). If you see that spam again, the `isatty()` gate regressed.
