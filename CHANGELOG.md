# Changelog

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
