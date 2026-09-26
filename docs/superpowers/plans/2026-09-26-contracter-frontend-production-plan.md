# CONTRACTER Frontend Production Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a production-quality, clickable CONTRACTER application without changing server authority or introducing fake market/live data.

**Architecture:** Extend the existing React/Vite router, AppShell, typed API client, and component system in vertical slices. Keep artwork, async states, and navigation owned by shared components; add no parallel frontend or wholesale UI dependency.

**Tech Stack:** React, TypeScript, Vite, Vitest, Testing Library, existing CSS token/component system.

**Spec:** `docs/superpowers/specs/2026-09-26-contracter-frontend-production-design.md`

## Global Constraints

- Preserve the existing routes and browser back/forward behavior.
- Keep server state authoritative; never fabricate prices, online counts, activity, roles, or skin artwork.
- CC remains the only runtime currency.
- No cases, roulette, slots, jackpots, fake near-misses, or gambling language.
- Reuse existing components and token files; do not add a large dependency for one interaction.
- Every behavior change gets a failing test before production code and a full test/build verification afterward.

## Review Focus

- Missing `canonical_image_url` must render explicit unavailable artwork, never another item’s image (Task 1).
- Steam callback failure must be understandable and must not expose secrets or raw provider/API payloads (Task 2).
- Unauthenticated, expired, loading, unavailable, and retry states must remain distinct (Task 2/3).
- Sparse or unavailable market valuation must disable purchase without inventing a price (Task 3).
- Reduced motion and keyboard navigation must preserve the same contract actions and destinations (Task 4).

### Task 1: Brand and artwork ownership

**Files:**
- Modify: `frontend/src/components/Logo.tsx`, `frontend/src/components/SkinCard.tsx`, `frontend/src/components/SkinImage.tsx`
- Modify: `frontend/src/visual-polish.css`, `frontend/src/image-states.css`, `frontend/DESIGN.md`
- Test: `frontend/src/components/Logo.test.tsx`, `frontend/src/components/SkinCard.test.tsx`, `frontend/src/components/SkinImage.test.tsx`

**Interfaces:**
- Consumes existing `Navigate`, `SkinDefinition`, `resolveSkinImage`, and CSS tokens.
- Produces a repository-owned logo component and a single artwork/fallback path for all skin cards.

- [ ] Write a failing logo test asserting the official mark structure/accessible label and route to `/contracts`.
- [ ] Run the focused logo test and verify it fails for the current CSS-only substitute.
- [ ] Implement the logo using an existing repository asset if available; otherwise add a minimal repository-owned SVG asset matching the approved two-card mark.
- [ ] Write a failing skin-card test asserting no decorative `.art-line` layer is rendered and that `SkinImage` owns artwork.
- [ ] Remove obsolete decorative layers and conflicting overrides without changing selection semantics.
- [ ] Run component tests and verify artwork fallback, loading, and selected states.
- [ ] Commit: `feat: make brand and artwork ownership explicit`.

### Task 2: Steam callback and auth recovery state

**Files:**
- Modify: `crates/api/src/routes/auth.rs` only if a redirect contract is required; otherwise modify `frontend/src/main.tsx`, `frontend/src/router.ts`, `frontend/src/login.css`
- Test: `frontend/src/main.test.tsx`, `frontend/src/router.test.ts`, and focused API/auth tests if backend changes are needed

**Interfaces:**
- Consumes existing `/api/v1/auth/steam/start`, `/api/v1/auth/steam/callback`, session state, and `Navigate`.
- Produces an explicit `steam-error` route/query state that explains failure and returns to login without leaking provider payloads.

- [ ] Add a failing test for a Steam callback error query rendering a safe login error and retry action.
- [ ] Run it and verify the current router/login has no explicit callback state.
- [ ] Implement the smallest route/query handling compatible with the current callback behavior; do not display raw API JSON.
- [ ] Add field-level validation, password visibility, busy state, and focus recovery only where existing auth forms lack them.
- [ ] Run auth/session tests and verify register → login → reload-shaped `/me` → logout and expired-session recovery.
- [ ] Commit: `feat: make auth callback recovery explicit`.

### Task 3: Market, inventory, and live-state consistency

**Files:**
- Modify: `frontend/src/components/LiveActivity.tsx`, `frontend/src/components/AppShell.tsx`, `frontend/src/pages/MarketPage.tsx`, `frontend/src/pages/InventoryPage.tsx`, `frontend/src/api.ts`
- Test: `frontend/src/components/LiveActivity.test.tsx`, `frontend/src/pages/MarketPage.test.tsx`, `frontend/src/pages/InventoryPage.test.tsx`, `frontend/src/api.test.ts`

**Interfaces:**
- Consumes typed catalog, valuation, inventory, balance, and history readers.
- Produces explicit `loading | connected | reconnecting | unavailable` states and preserves disabled purchase behavior when valuation is unavailable.

- [ ] Add failing tests for public unauthenticated live state, sparse valuation, unavailable purchase, and retry.
- [ ] Run focused tests and verify the missing behavior.
- [ ] Implement only server-backed activity/online integration; if no endpoint exists, render an explicit unavailable state.
- [ ] Keep market pagination/filter/sort and inventory refresh behavior unchanged while making states visually consistent.
- [ ] Run the full frontend test suite and build.
- [ ] Commit: `feat: clarify market and live data states`.

### Task 4: Contract reveal and responsive/accessibility polish

**Files:**
- Modify: `frontend/src/components/ContractReveal.tsx`, `frontend/src/components/ContractBuilder.tsx`, `frontend/src/reveal-cinematic.css`, `frontend/src/accessibility.css`, `frontend/src/layout-fixes.css`, `frontend/src/components/AppShell.tsx`
- Test: `frontend/src/components/ContractReveal.test.tsx`, `frontend/src/components/ContractBuilder.test.tsx`, `frontend/src/components/AppShell.test.tsx`

**Interfaces:**
- Consumes existing quote/accept callbacks and selected inventory items.
- Produces a staged contract-only reveal with reduced-motion fallback and stable keyboard/focus behavior.

- [ ] Add failing tests for reveal stage progression, reduced-motion behavior, and keyboard focus.
- [ ] Run focused tests and verify they fail for the current timing/state behavior.
- [ ] Implement staged selection → composition → commitment → result motion without case/roulette visual language.
- [ ] Verify narrow viewport navigation, focus rings, labels, and error/retry controls.
- [ ] Run all frontend tests and production build.
- [ ] Commit: `feat: refine contract reveal and responsive interaction states`.

### Task 5: Final verification and evidence

**Files:**
- Modify: `docs/LAUNCH_CHECKLIST.md`, `frontend/build-evidence.txt` only with current evidence

- [ ] Run `npm test --prefix frontend -- --run`.
- [ ] Run `npm run build --prefix frontend`.
- [ ] Run repository formatter/linter checks.
- [ ] Exercise direct routes, browser back/forward, loading/error/empty states, keyboard navigation, and reduced motion.
- [ ] Run the premium static audit and resolve blocking findings.
- [ ] Update only evidence-backed checklist entries.
- [ ] Commit: `docs: record frontend verification evidence`.

