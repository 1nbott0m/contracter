# Contracter Frontend Redesign Design

## Goal

Turn the existing React/Vite frontend into a coherent Contracter product shell
with a live, high-energy contract stage and calm market, inventory, history,
profile and admin workspaces, inspired by interaction patterns from the supplied
references but using original Contracter identity and copy.

## Scope

In scope: shared shell, visual tokens, responsive layout, item cards, market and
inventory presentation, history presentation, contract selection/reveal motion,
loading/empty/error states, accessible dialogs and navigation consistency.

Out of scope: backend API changes, real-money payments, competitor assets or
copy, and fabricated online counts or activity. Existing CC and owner-bound API
contracts remain authoritative.

## Design direction

Graphite canvas, cyan primary action, violet secondary accent, rounded panels,
stable image slots, and a single staged reveal animation. The contract route is
the expressive surface; operational routes prioritize scanability and clear
ownership. The full token and motion rationale lives in the repository-root
`DESIGN.md`.

## Behavior contract

1. `AppShell` owns the same header, live status, profile affordance and route
   navigation on desktop and mobile.
2. Every async route renders explicit loading, empty, error and retry states;
   production never substitutes development fixtures.
3. Contract submission is disabled until 4–10 owned items are selected. A
   successful result returns to the contract stage and offers history/details.
4. Reveal motion is skippable and reduced-motion safe; it never changes the
   server result or exposes secrets.
5. Market purchase, inventory actions and admin actions retain existing API
   authorization and idempotency behavior.
6. Destructive/irreversible actions use an app-owned accessible dialog; status
   messages use a shared live-region/toast owner.

## Technical approach

Keep React/Vite and existing API modules. Consolidate visual values into CSS
custom properties imported by the existing stylesheet graph, then migrate
shared components before route-specific styling. Reuse `lucide-react`; do not
add a UI framework for this pass. Add focused component tests for state and
keyboard behavior, then validate desktop and mobile renders in the browser.

## Acceptance criteria

- All primary routes use the same shell and responsive navigation.
- Contract reveal visually reads as gather → fuse → reveal → result and has a
  skip/reduced-motion path.
- Cards have stable image geometry, readable text and visible focus states.
- No overlapping decorative layers obscure skin artwork or controls.
- `npm test -- --run` and `npm run build` pass.
- Browser smoke checks cover route identity, no overlay, console health,
  screenshot evidence and one interaction on desktop and mobile.
