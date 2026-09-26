# CONTRACTER Frontend Design Context

## North star

CONTRACTER is a dark, precise CS2 contract workspace: evidence-led, server-truthful, and cinematic without hiding operational state.

## Visual language

- Near-black canvas, graphite panels, cyan verification accents, and restrained rarity color.
- Compact uppercase labels support a dense market-tool feel; body copy remains readable Russian text.
- Cards use consistent rounded corners, visible focus rings, and stable image slots so async artwork never shifts controls.
- Motion communicates loading, selection, reveal, and success; `prefers-reduced-motion` removes decorative motion.
- Motion tokens are shared across the product: instant `90ms`, fast `180ms`, normal `320ms`, emphasis `620ms`; standard easing is `cubic-bezier(.2,.8,.2,1)` and signature reveal easing is `cubic-bezier(.16,1,.3,1)`. Frequent controls use brief transform/opacity feedback; only contract commit/reveal receives emphasis motion.
- Typography uses a performance-safe system UI stack with a monospace numeric role for IDs and CC values; no remote font dependency is required for first paint or Cyrillic fallback.

## Runtime token ownership

The existing CSS token files are canonical runtime adapters: `design-tokens.css`, `accessibility.css`, and the shared component styles. New screens reuse those tokens instead of introducing a parallel theme.

## Interaction principles

- Server state is authoritative. Missing catalog prices, unavailable APIs, and empty inventories are stated plainly and never replaced with fake production data.
- Navigation uses native links/buttons with keyboard-visible focus and route-local headings.
- Market and inventory filters use intentionally native selects for short OS-owned option lists; forms own their validation copy with `noValidate`.

## External research record (2026-09-26)

External references were inspected before the production redesign:

- Refero **Ramp** style reference: [styles.refero.design/style/b38702a0-75ab-474c-9106-00b624535825](https://styles.refero.design/style/b38702a0-75ab-474c-9106-00b624535825)
- Refero **Raycast** style reference: [styles.refero.design/style/3b6a17f0-3bdf-418c-a95e-0b89e5a8b2f8](https://styles.refero.design/style/3b6a17f0-3bdf-418c-a95e-0b89e5a8b2f8)
- Refero **Dala** dark-stage reference: [styles.refero.design/style/e5f5f8cf-e68d-4ed1-bbf5-6b67569af648](https://styles.refero.design/style/e5f5f8cf-e68d-4ed1-bbf5-6b67569af648)
- Skiper component catalogue: [skiper-ui.com/components](https://skiper-ui.com/components)

### Principles adopted

- From Ramp: use hairline borders, restrained surface hierarchy, 4/8/12/16/24 spacing rhythm, and reserve a vivid accent for meaningful state changes.
- From Raycast: treat the dark product shell as a power-tool cockpit, use quiet neutral surfaces, keyboard-first command interactions, and compact technical labels.
- From Dala: keep one intentional signature moment and avoid filling an operational interface with decorative particles or unrelated imagery.
- From Apple's HIG motion guidance: custom motion must be purposeful, brief, precise, cancelable, and never the only channel for important state. From Motion's transition model: use short standard transitions for routine feedback and reserve spring-like emphasis for the signature reveal; CONTRACTER implements this with CSS tokens to avoid a runtime animation dependency.

These principles are translated into CONTRACTER's own graphite/cyan/violet
tokens. Their palettes, logos, copy, typography, and proprietary imagery are
not copied.

### Skiper evaluation

- **Considered:** Command Palette (Skiper92), expandable tabs (Skiper96),
  tooltip menu (Skiper43), and mouse-follow motion (Skiper61).
- **Accepted as inspiration only:** command palette interaction and subtle
  tooltip/keyboard affordances. CONTRACTER's existing `GlobalSearch` and
  shared primitives remain the implementation owners, avoiding a new runtime
  dependency and preserving route/API behavior.
- **Rejected:** Skiper92/96/43 source installation because the available
  versions require Pro access or introduce `framer-motion`/measurement
  dependencies that are not justified for the current flows. Skiper61 mouse
  tracking is rejected because it adds decorative motion to inventory cards and
  conflicts with reduced-motion and precision-tool goals.

Skiper's own terms distinguish free components (commercial use with
attribution) from Pro components that require a license; no paid component or
license key is used here.

### Premium motion review

- Level 1 micro feedback: focus rings, button press scale, card hover lift.
- Level 2 navigation/panels: page and modal opacity/translate transitions.
- Level 3 product actions: selection, inventory add, market purchase status.
- Level 4 signature moment: contract commit → staged merge → server result.

Every level has a non-motion semantic state, and the global reduced-motion
rule removes decorative loops while preserving labels, status text, and actions.
