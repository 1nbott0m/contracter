# CONTRACTER 100-Point Completion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bring the existing CONTRACTER frontend/backend deployment to a verified state for the 100-item scope in `pasted-text-1.txt`, preserving server authority, CC accounting, owner isolation, and safe failure behavior.

**Architecture:** Work in independent vertical slices. First fix the production-visible market loading path, then prove auth/core contract flows through HTTP and browser-shaped tests, then harden admin/security/economy/importer and finally run the operational audit. Existing Rust APIs and React/Vite frontend remain the stack; new behavior must extend existing typed readers and shared components rather than add parallel implementations.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, React, TypeScript, Vite, Vitest, GitHub Actions, Render, Cloudflare Pages.

**Spec:** `docs/frontend/FRONTEND_AUDIT.md`, root `DESIGN.md`, `frontend/DESIGN.md`, and `/Users/yan/.codex/attachments/06dbd772-e977-4093-8527-42db9278d6fe/pasted-text-1.txt`.

## Global Constraints

- Server responses remain authoritative; no production demo data or client-side role authority.
- CC is the only internal currency; do not reintroduce ruble-valued runtime fields.
- Preserve the 20 CC minimum, 15,000 RUB-equivalent cap, owner isolation, idempotency, and append-only audit invariants.
- Secrets, quote seeds, TOTP secrets, API keys, and database credentials never enter frontend bundles, URLs, logs, or user-visible errors.
- Every changed flow needs a focused regression test and a proportionate production-shaped verification.
- Do not add shadcn/Skiper wholesale; prefer the existing Contracter-owned primitives unless a concrete accessibility need justifies a focused dependency.

## Review Focus

- Large catalog with only a small published valuation set must render progressively and distinguish unavailable prices from API failure.
- Cross-origin cookies must survive login, reload, logout, and expired-session recovery.
- Repeated/concurrent purchase and quote-accept requests must remain idempotent and never create negative balances.
- Non-admin, stale-TOTP, malformed importer data, and unavailable source states must fail closed.
- Reduced-motion, keyboard focus, mobile navigation, and screen-reader status must remain usable after visual changes.

### Task 1: Production market loading (P0)

**Files:** `frontend/src/api.ts`, `frontend/src/pages/MarketPage.tsx`, related tests; backend market/catalog route only if evidence proves an API contract change is required.

- [ ] Add a typed bounded-page reader for the initial market view and a progressive “load more” path; do not fetch 6,821 rows before first render.
- [ ] Preserve unavailable valuation state per SKU and show the first usable page when valuations are sparse.
- [ ] Add tests for 35-page catalog, partial valuations, repeated cursor, and first-render success.
- [ ] Reproduce the published browser route and verify visible cards and filters.

### Task 2: Auth and session proof (P0)

**Files:** `frontend/src/main.tsx`, `frontend/src/session.ts`, auth tests, browser smoke harness.

- [ ] Add field-level validation, password visibility, busy/focus recovery, and explicit Steam callback failure state.
- [ ] Verify register → login → reload → `/me` → logout and expired-session recovery against production-shaped responses.
- [ ] Add CORS/cookie regression coverage without exposing credentials.

### Task 3: Core catalog/inventory/market behavior (P0)

**Files:** typed API readers, market/inventory pages, shared data-list primitives, tests.

- [ ] Prove inventory/catalog pagination, filters, sorting, unavailable purchase, successful purchase, idempotency, balance refresh, and inventory refresh.
- [ ] Make loading/empty/error/retry states explicit and non-fabricated.

### Task 4: Contract lifecycle and history (P0)

**Files:** `ContractsPage`, `ContractBuilder`, `ContractReveal`, typed history readers, backend contract/history routes only where missing.

- [ ] Prove allocation, quote creation, expiry, accept, 4-item, 10-item, 3-item rejection, 11-item rejection, stale quote, duplicate accept, owner isolation, and negative-balance protection.
- [ ] Add server-backed contract detail/history projections for inputs, result, values, ledger and inventory events.
- [ ] Verify 20 CC floor, 15,000 RUB-equivalent cap, commitment, signature and all result-delivery paths.

### Task 5: Admin and security (P1)

**Files:** admin route/components, auth/admin tests, security middleware/tests, deployment docs.

- [ ] Prove server-side role checks, non-admin denial, TOTP provisioning/verification/retry, admin session/logout, audit, metrics and user listing.
- [ ] Verify rate limits, CORS, HSTS boundary, security headers, CSRF boundary, request IDs, secret redaction, seed/API-key absence, SQL privilege separation, append-only audit and backup/restore procedure.

### Task 6: Economy and concurrency (P1)

**Files:** Rust domain/database code, migrations and integration tests.

- [ ] Run fresh PostgreSQL integration suite for ledger atomicity, reservation rollback, duplicate operations, concurrent purchases/acceptance/selection, stale quotes, scarcity, rarity, wear, StatTrak, Souvenir, float, decimal arithmetic, CC conversion, rounding, floor/cap, signatures, risk history, reconciliation and migration checksums.
- [ ] Run load tests and document any environment-only limitation instead of weakening invariants.

### Task 7: Market importer (P1)

**Files:** importer worker/workflow, SQL snapshot publication, importer tests/docs.

- [ ] Prove timeout, HTTP 500/520, malformed/empty/duplicate evidence, timestamps, currency conversion, hashes, sale count, publication, rollback, idempotency, manual/scheduled workflows, failure notification, stale halt, disabled source and frontend unavailable valuation behavior.

### Task 8: UX, accessibility and launch operations (P2)

**Files:** design tokens, shared components, CSS, pages, docs and CI.

- [ ] Verify images/fallbacks, logo, responsive shell, profile, card layouts, cinematic reveal timing/stages/smoke/reduced motion, skeletons, errors/retries, keyboard/focus/screen reader/contrast/toasts, purchase/expiry/result/history media and legal/information links.
- [ ] Verify live/no-fake-data/cache behavior, bundle/cache headers, Render cold start, health/readiness/metrics/logs, dependency/image audits, non-root runtime, rollback/migration policy, runbooks, environment docs, CI permissions, branch protection, tags, deployment smoke and version compatibility.

## Execution and gates

Each task is implemented in a separate change set and must pass its focused tests before the next task. The P0 market fix is the first implementation task because it is the only confirmed production-visible blocker. A final 100-item matrix will map each item to evidence; items requiring unavailable external credentials or paid infrastructure will be marked explicitly as unverified rather than claimed complete.
