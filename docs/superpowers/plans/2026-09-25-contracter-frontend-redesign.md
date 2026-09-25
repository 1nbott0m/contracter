# Contracter Frontend Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship the approved Contracter visual system and coherent multi-route frontend without changing backend contracts.

**Architecture:** Keep the current React/Vite routes and API adapter. Establish CSS tokens and shared shell/primitives first, then migrate contract reveal and data routes onto those owners. Preserve the existing session and API state boundaries; add no payment or fabricated activity behavior.

**Tech Stack:** React 19, TypeScript, Vite, Vitest, Testing Library, lucide-react, CSS custom properties.

**Spec:** `docs/superpowers/specs/2026-09-25-contracter-frontend-redesign-design.md`

## Global Constraints

- Internal currency remains CC; no client-controlled money or payment behavior.
- Existing API routes and authorization contracts are unchanged.
- Production must never use development fixtures.
- Every clickable control has keyboard focus, hover/pressed and disabled/busy states.
- Every async route has loading, empty, error and retry states.
- Motion honors `prefers-reduced-motion` and can be skipped.
- Run frontend tests and build before completion.

## Review Focus

- A contract with 3, 11 or foreign-owned items must remain blocked; test in `ContractBuilder.test.tsx`.
- Reveal skip and reduced-motion must settle on the same server result; test in `ContractReveal.test.tsx`.
- A failed market/inventory/history request must not replace live data with fixtures; test the existing page suites.
- Mobile navigation must not cover the primary action or trap document scrolling; verify with browser smoke and `AppShell.test.tsx`.
- Missing skin artwork must preserve the media slot and readable metadata; test `SkinImage.test.tsx` and browser screenshot.

### Task 1: Design context and token foundation

**Files:**
- Create: `DESIGN.md`
- Create: `docs/superpowers/specs/2026-09-25-contracter-frontend-redesign-design.md`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/src/accessibility.css`
- Test: `frontend/src/components/AppShell.test.tsx`

- [ ] Add CSS variables matching `DESIGN.md` and replace repeated hard-coded surface, text, accent, radius and spacing values in shared shell rules.
- [ ] Add global focus-visible, scrollbar and reduced-motion rules without hiding scrollbars or changing backend behavior.
- [ ] Extend shell tests for route navigation labels and visible focus affordances.
- [ ] Run `npm test -- --run frontend/src/components/AppShell.test.tsx` and fix failures.

### Task 2: Shared shell and primitives

**Files:**
- Modify: `frontend/src/components/AppShell.tsx`
- Modify: `frontend/src/components/Footer.tsx`
- Modify: `frontend/src/components/Logo.tsx`
- Modify: `frontend/src/components/SkinCard.tsx`
- Create: `frontend/src/components/StatusChip.tsx`
- Create: `frontend/src/components/Panel.tsx`
- Test: `frontend/src/components/AppShell.test.tsx`, `Footer.test.tsx`, `SkinCard.test.tsx`

- [ ] Keep one desktop header and one mobile bottom navigation owner; remove duplicate or misleading footer route labels.
- [ ] Build `Panel` and `StatusChip` from shared tokens and semantic variants.
- [ ] Give `SkinCard` stable image dimensions, rarity text, CC value, ownership state and keyboard interaction.
- [ ] Preserve profile, logout and route navigation behavior.
- [ ] Run the three focused test files.

### Task 3: Contract stage and cinematic reveal

**Files:**
- Modify: `frontend/src/components/ContractBuilder.tsx`
- Modify: `frontend/src/components/ContractReveal.tsx`
- Modify: `frontend/src/components/ContractSummary.tsx`
- Modify: `frontend/src/contract-builder.css`
- Modify: `frontend/src/reveal-cinematic.css`
- Test: `frontend/src/components/ContractBuilder.test.tsx`, `ContractReveal.test.tsx`

- [ ] Implement four explicit visual states: gather, fuse, reveal and settled result.
- [ ] Add a visible skip action and ensure both skip and reduced motion resolve to the same result state.
- [ ] Keep 4–10 selection validation, API call, error recovery and result navigation unchanged.
- [ ] Remove decorative overlays that cover skin artwork; reserve the contract beam for the stage background.
- [ ] Run focused contract tests and add assertions for skip/reduced motion.

### Task 4: Market, inventory and history workspaces

**Files:**
- Modify: `frontend/src/pages/MarketPage.tsx`
- Modify: `frontend/src/pages/InventoryPage.tsx`
- Modify: `frontend/src/pages/HistoryPage.tsx`
- Modify: `frontend/src/commerce.css`
- Modify: `frontend/src/information.css`
- Test: the existing Market, Inventory and History page suites

- [ ] Migrate cards, filters, cursor/load-more controls and state messaging to shared panels and tokens.
- [ ] Keep CC values explicit and avoid implying fiat deposits.
- [ ] Preserve owner isolation, API errors and retry behavior.
- [ ] Run the three page suites.

### Task 5: Profile, auth and admin polish

**Files:**
- Modify: `frontend/src/main.tsx`
- Modify: `frontend/src/login.css`
- Modify: `frontend/src/ux.css`
- Modify: `frontend/src/pages/ProfilePage.tsx`
- Test: `frontend/src/main.test.tsx`, `ProfileMenu.test.tsx`, dialog accessibility tests

- [ ] Apply the shell to login, registration, profile and admin without exposing secrets or changing auth flows.
- [ ] Use app-owned accessible dialogs/status messages for 2FA and account actions.
- [ ] Preserve Steam redirect, session expiration and admin TOTP behavior.
- [ ] Run auth/profile/accessibility tests.

### Task 6: Browser QA and release verification

**Files:**
- Modify only files required by findings from Tasks 1–5.
- Do not commit screenshots or temporary browser scripts.

- [ ] Run `npm test -- --run`.
- [ ] Run `npm run build`.
- [ ] Start the frontend and inspect `/contracts`, `/market`, `/inventory`, `/history`, `/profile` at desktop and mobile widths.
- [ ] Verify page identity, non-blank content, no framework overlay, console health, screenshot evidence and one interaction per target flow.
- [ ] Run `git diff --check` and record the final commit.
