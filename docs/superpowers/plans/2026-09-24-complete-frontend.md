# CONTRACTER complete frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current Vite/React frontend into a real, responsive, multi-route CONTRACTER application while preserving the existing backend API and the approved cinematic contract-signing flow.

**Architecture:** Keep the existing Vite React TypeScript app, but split the current monolithic `main.tsx` into shared shell, typed route/page, data, and component modules. Keep one canonical skin model and one API adapter; isolate development fallback data from production responses. Preserve the cinematic reveal as contract display frames, never case-opening mechanics.

**Tech Stack:** React 19, TypeScript, Vite, lucide-react, existing Rust API, CSS modules/files, browser History API.

**Spec:** `docs/superpowers/specs/2026-09-24-complete-frontend-design.md`

## Global Constraints

- Real canonical skin artwork is required wherever a skin is shown.
- No cases, crates, roulette, slot-machine, jackpot, near-miss, or gambling language.
- Production online/activity/balance/inventory/history values must come from API or be explicitly unavailable.
- Every visible control must navigate, filter, search, submit, or explain its unavailable backend capability.
- Every async view has loading, success, empty, and error states.
- Preserve `prefers-reduced-motion`, keyboard focus, semantic controls, and mobile layouts.

## Review Focus

- Direct refresh on every required route must render the correct page: route smoke tests in Task 2.
- API unavailable or unauthenticated must never leave fake production data visible: adapter/state tests in Task 3.
- Search/filter/selection must remain consistent across market, inventory, and contract builder: component tests in Tasks 4–5.
- Reveal must not imply case opening and must not hide the server result behind a fake result: reveal tests in Task 6.
- Mobile viewport must keep navigation and contract actions usable: responsive browser checks in Task 8.

### Task 1: Extract shared design and skin primitives

**Files:** Create `frontend/src/types.ts`, `frontend/src/components/SkinImage.tsx`, `frontend/src/components/SkinCard.tsx`, `frontend/src/components/Logo.tsx`; modify `frontend/src/api.ts`, `frontend/src/mocks/dev-data.ts`, `frontend/src/main.tsx`.

- [ ] Define `SkinDefinition`, `InventoryItem`, `HistoryItem`, `MarketItem`, and typed async state in `types.ts`.
- [ ] Move canonical image/error/loading behavior into `SkinImage` with reserved dimensions and fallback artwork.
- [ ] Move duplicated card markup into `SkinCard` and preserve add/remove actions.
- [ ] Make `Logo` a reusable link to `/contracts`.
- [ ] Run `npm --prefix frontend run build` and commit.

### Task 2: Add real routing and shared application shell

**Files:** Create `frontend/src/router.ts`, `frontend/src/components/AppShell.tsx`, `frontend/src/pages/NotFoundPage.tsx`; modify `frontend/src/main.tsx`, `frontend/src/styles.css`.

- [ ] Implement route matching for `/contracts`, `/market`, `/inventory`, `/history`, `/contracts/:id`, `/profile`, `/login`, `/transparency`, `/terms`, `/privacy`, `/support`.
- [ ] Preserve browser back/forward and direct refresh behavior.
- [ ] Move live bar, header, footer, and mobile bottom navigation into `AppShell`.
- [ ] Add a real 404 page and active-route state.
- [ ] Run build and direct URL smoke checks for every route; commit.

### Task 3: Centralize API/session/loading/error state

**Files:** Modify `frontend/src/api.ts`; create `frontend/src/session.ts`, `frontend/src/hooks/useAsync.ts`, `frontend/src/hooks/useSession.ts`.

- [ ] Add typed API methods for catalog, inventory, history, contract details, login/logout, market purchase, and verification using existing endpoints only.
- [ ] Add session loading, authenticated, unauthenticated, expired, and error states.
- [ ] Ensure fallback data is used only when explicitly in development mode and is never presented as production activity or online count.
- [ ] Build and test API failure states; commit.

### Task 4: Complete contracts page and canonical contract builder

**Files:** Create `frontend/src/components/ContractBuilder.tsx`, `ContractSummary.tsx`, `PossibleResults.tsx`; create/modify `frontend/src/pages/ContractsPage.tsx`; modify styles.

- [ ] Implement 4–10 selection bounds, add/remove/inspect actions, totals, empty slots, and disabled/ready/submitting/error CTA states.
- [ ] Render possible results only with real images and no invented probabilities.
- [ ] Preserve API response as the only source of committed result data.
- [ ] Add selection/add/remove regression tests and build.

### Task 5: Build real market, inventory, search, and profile flows

**Files:** Create `MarketPage.tsx`, `InventoryPage.tsx`, `ProfilePage.tsx`, `GlobalSearch.tsx`, `ProfileMenu.tsx`, `InventoryFilters.tsx`; modify API/types/router.

- [ ] Add market search, filters, sorting, purchase loading/success/error states, and balance/inventory refresh.
- [ ] Add inventory search, weapon/rarity/wear/price filters, sorting, and add-to-contract motion.
- [ ] Add global keyboard search with Arrow Up/Down, Enter, and Escape.
- [ ] Add profile dropdown, login redirect, logout, session state, inventory/history shortcuts.
- [ ] Build and smoke-test all controls; commit.

### Task 6: Implement history, contract details, verification, and contract reveal

**Files:** Create `HistoryPage.tsx`, `ContractDetailsPage.tsx`, `ContractReveal.tsx`, `VerificationPage.tsx`; modify router/API and existing reveal CSS.

- [ ] Render history rows/cards with input images, result image, values, status, and links to `/contracts/:id`.
- [ ] Render contract details without confidential cryptographic or economic internals.
- [ ] Implement a private owner-history lookup route; do not label it public verification or expose cryptographic internals until a dedicated backend endpoint exists.
- [ ] Rework reveal to `SELECT → LOCK → SIGN → MERGE → RESULT`: display frames are contract panels, not cases/crates; use real selected images and server result artwork; keep the approved slow cinematic pacing.
- [ ] Add reduced-motion fallback and test result/error paths; commit.

### Task 7: Add transparency, legal, support, footer, and live activity

**Files:** Create `TransparencyPage.tsx`, `TermsPage.tsx`, `PrivacyPage.tsx`, `SupportPage.tsx`, `LiveActivity.tsx`, `Footer.tsx`; modify `AppShell` and API.

- [ ] Add real pages for transparency, terms, privacy, and support; no legal claims beyond known project facts.
- [ ] Make every footer link functional and remove placeholder anchors.
- [ ] Render live activity with real skin images when API data exists; otherwise show explicit unavailable/empty state.
- [ ] Add online loading/connected/reconnecting/unavailable states without fake production counts.
- [ ] Build and route-smoke-test; commit.

### Task 8: Responsive, accessibility, performance, and final verification

**Files:** Modify component styles and `frontend/index.html`; create `frontend/src/error-boundary.tsx` and focused tests under `frontend/src/__tests__/`.

- [ ] Verify 375, 390, 430, 768, 1024, and 1440px layouts.
- [ ] Add semantic labels, focus states, Escape dialog close, and image alt text.
- [ ] Confirm lazy loading, reserved image dimensions, no console errors, and no dead buttons.
- [ ] Run `npm --prefix frontend run build`, `git diff --check`, route smoke tests, and production URL verification.
- [ ] Commit final frontend and publish the production deployment.
