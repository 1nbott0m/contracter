# CONTRACTER launch checklist

This checklist is evidence-driven. Do not mark a line complete from intent alone.

## Automated gates

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] workspace tests and PostgreSQL integration in GitHub Actions
- [x] frontend tests and production build
- [x] `cargo audit` and production `npm audit`
- [x] deployment manifest safety checks
- [x] backup/restore guard checks
- [x] concurrent market purchase and inventory-selection tests
- [x] concurrent quote acceptance test

## Production smoke

- [x] Cloudflare frontend returns HTTP 200
- [x] frontend bundle points at the Render API
- [x] Render `/health/live` returns 200
- [x] Render `/health/ready` returns 200
- [x] registration, login, `/me`, logout, and post-logout 401 verified
- [x] disallowed Origin returns 403
- [x] security response headers observed on the API
- [ ] Steam OAuth production callback verified with the real Steam application
- [ ] importer publishes a confirmed valuation snapshot
- [ ] market purchase verified with published production valuation

## Required operator actions before public launch

- [ ] configure and rotate production secrets in the hosting provider
- [ ] assign the two approved administrators and enroll both TOTP devices
- [ ] verify Steam redirect URL and callback environment variables
- [ ] configure `MARKET_CSGO_API_KEY` and `MARKET_IMPORT_DATABASE_URL` in GitHub Actions
- [ ] run importer dry ingest, review evidence count, then run `--publish`
- [ ] complete a non-production backup/restore drill
- [ ] run the k6 smoke scenario against staging only
- [ ] record the deployment commit, migration state, and rollback reference

## Explicit launch blockers

Do not enable market prices or purchases while valuations are `available:false`.
Do not run remote load tests against production. Do not commit credentials or
database URLs. Payment integration is outside this checklist until its provider,
webhook signing, and settlement policy are specified.
