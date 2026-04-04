# Changelog

## [0.1.7] - 2026-04-04

### Fixed
- Source map and release uploads via sentry-cli
- Upload chunk isolation and write safety

## [0.1.6] - 2026-04-04

### Added
- Per-project API keys for source map and release uploads (`spk_` prefix, SHA-256 hashed)
- Source Maps settings page with key generation, setup guide for `sentry-cli`
- API key auth on all Sentry-compatible upload endpoints (`/api/0/`)

### Fixed
- Project "First Seen" no longer changes with the time filter
- "All time" filter now propagates correctly to project detail views

### Added
- Browser defaults: configurable default filters (status, level, period) stored as a cookie
- Settings page at `/web/settings/defaults/` to manage defaults
- List pages redirect to fill in missing filter params from saved defaults

## [0.1.5] - 2026-04-03

### Changed
- Event detail page queries run concurrently instead of sequentially
- Event navigation (prev/next/total) consolidated from 3 queries to 1
- Project list replaces correlated subquery with a pre-aggregated JOIN
- Tag facet lookups covered by a new composite index
- CSRF middleware rejects missing cookie before consuming request body
- ORDER BY columns use static pushes instead of format interpolation

### Fixed
- SVG sanitizer no longer re-allocates on each loop iteration
- Partial sentry keys no longer logged on auth failure
- Infallible parse calls use `expect` instead of `unwrap`

## [0.1.4] - 2026-04-02

### Fixed
- SVG text escaping order causing double-encoded ampersands in chart labels
- SVG sanitizer bypasses for unquoted attribute values and long handler names
- Missing master key now blocks startup if encrypted integration secrets exist
- Compression failures no longer corrupt stored events (read path falls back to raw JSON)
- zstd compression no longer blocks the async runtime (`block_in_place`)
- `WriteError` now implements `std::error::Error` for proper error chain composition

### Changed
- Event severity `level` field is now a typed enum instead of a free-form string
- Nav badge counts consolidated from 9 subqueries to a single events table scan
- Filtered events now return `X-Sentry-Discarded` header so operators can detect drops

## [0.1.3] - 2026-03-12

### Fixed
- RPM build failing due to missing `package.metadata.generate-rpm` config
- GitHub Actions Node.js 20 deprecation warnings (checkout v5, artifact actions v6)
- Added explicit deb packaging metadata

## [0.1.2] - 2026-03-12

### Added
- Logout mechanism with nav bar button
- Security headers on all admin responses (CSP, X-Frame-Options, X-Content-Type-Options, Referrer-Policy)
- Login-specific rate limiting (10/min per IP, separate from general admin limit)
- Periodic eviction of stale notification rate limiter entries

### Fixed
- CSRF cookie no longer set as HttpOnly, allowing JS double-submit injection to work
- Login cookie stores a SHA-256 derivative instead of the raw admin token
- CSRF body size limit now uses configured `max_body_size` instead of hardcoded 10MB
- Notification rate limiter no longer wastes per-project budget when global limit rejects
- Discard stats flush no longer double-counts on partial DB write failures
- Threshold alert state update failures are now logged instead of silently dropped

## [0.1.1] - 2026-03-09

### Added
- Landing page for the ingest port

### Changed
- Updated dependencies (`rand`, `reqwest`, `zip`, `toml`)

## [0.1.0] - 2026-03-08

### Added
- Initial release
- Sentry-compatible error tracking and event ingestion
- SQLite and PostgreSQL support
- Web dashboard with project management
- Source map processing
- CSRF protection
- Webhook notifications
