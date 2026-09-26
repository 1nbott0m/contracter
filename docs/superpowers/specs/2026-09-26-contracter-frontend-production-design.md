# CONTRACTER Frontend — production application design

## Goal

Turn the existing React/Vite frontend into a coherent, clickable, responsive
CONTRACTER application while preserving the existing API contracts, server
authority, CC-only economy, owner isolation, and honest unavailable states.

This is an application redesign, not a landing page and not a case-opening
experience.

## Product contract

The primary flow is:

`select 4–10 items → build contract → review → commit → reveal → one result`.

The UI must explain that sequence and must never introduce cases, roulette,
jackpots, fake near-misses, random balances, fake online counts, or fabricated
market prices.

## Visual contract

The design language is **Precision Graphite**:

- canvas `#090A0C`, secondary `#0D0F12`, surfaces `#12151A`;
- borders `#242932`, primary text `#F4F5F7`, secondary text `#8B929E`;
- cyan `#58D6E7` means interaction/selection;
- violet `#8B5CF6` means rarity/result/brand moment;
- green `#63D69A` and red `#E06C75` are semantic status colors;
- graphite remains the dominant visual field, with artwork supplying color.

The shared token files remain the runtime source of truth. No page-local theme
or wholesale shadcn/Skiper dependency is introduced.

## Application shell

All application routes use one `AppShell` containing:

`live activity → header/navigation → route content → footer`.

The header owns logo, primary navigation, search, honest online state, CC
balance state, and profile menu. The footer owns information, legal, support,
and fairness links. Native links remain usable with modified-click and browser
back/forward behavior.

## Data and trust boundaries

- API/catalog data is authoritative for names, images, prices, inventory,
  history, and contract results.
- Missing valuation remains unavailable; no client-side price is invented.
- Missing artwork uses the explicit unavailable-art fallback and never another
  skin's image.
- Online presence and public live activity are rendered only when a real API
  source exists; otherwise the UI says unavailable or authentication required.
- Session and admin role remain server-authorized.
- Secrets, TOTP values, quote seeds, and API keys never enter the bundle or
  URL.

## P0 implementation slices

1. Update the maintained frontend design context and token mapping.
2. Replace the CSS-only logo substitute with the repository's official logo
   asset when present; otherwise add a repository-owned asset matching the
   approved two-card mark, without AI-generated replacement artwork.
3. Remove obsolete decorative skin-card layers and keep `SkinImage` as the
   only artwork/fallback owner.
4. Add a safe Steam callback result route/state so verification failures are
   understandable in the frontend and successful callbacks land in the
   authenticated contract application.
5. Verify auth/session loading, error, expired, and retry states against the
   existing typed API client.

## P1 implementation slices

1. Use a server-backed live activity endpoint if one exists; otherwise keep an
   explicit unavailable state and document the integration point.
2. Keep contract details and history server-backed, including result and event
   projections where the backend exposes them.
3. Extend global search into a keyboard-accessible command surface while
   preserving the existing market search route.
4. Improve responsive header behavior so search, balance state, and profile
   remain discoverable on narrow screens.
5. Consolidate conflicting CSS overrides into the existing token/component
   ownership path.
6. Make unavailable purchase, loading, empty, retry, and success states
   consistent across market and inventory.

## P2 implementation slices

1. Implement a staged contract reveal: selection, composition, commitment,
   result. It must remain a contract reveal, not a case/roulette animation.
2. Add reduced-motion and duration regression coverage.
3. Complete route-by-route keyboard, focus, screen-reader, contrast, and
   narrow viewport verification.
4. Add bundle/cache checks to CI and finish legal/support/footer content review.

## Verification contract

For every slice run:

- frontend unit/component tests;
- TypeScript build;
- existing repository formatter/linter checks;
- a browser-shaped route check for success, loading, empty, error, keyboard,
  and reduced-motion states where applicable.

No slice is marked complete merely because the build passes. Each completed
slice must cite the specific test or runtime evidence that proves its behavior.

## Explicit non-goals

- No backend rewrite when an endpoint already exists.
- No fake market values, fake online numbers, fake activity, or client role
  authority.
- No cases, loot boxes, roulette, wheel, slot, jackpot, or gambling language.
- No wholesale component-library migration.
