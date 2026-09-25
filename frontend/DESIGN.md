# CONTRACTER Frontend Design Context

## North star

CONTRACTER is a dark, precise CS2 contract workspace: evidence-led, server-truthful, and cinematic without hiding operational state.

## Visual language

- Near-black canvas, graphite panels, cyan verification accents, and restrained rarity color.
- Compact uppercase labels support a dense market-tool feel; body copy remains readable Russian text.
- Cards use consistent rounded corners, visible focus rings, and stable image slots so async artwork never shifts controls.
- Motion communicates loading, selection, reveal, and success; `prefers-reduced-motion` removes decorative motion.

## Runtime token ownership

The existing CSS token files are canonical runtime adapters: `design-tokens.css`, `accessibility.css`, and the shared component styles. New screens reuse those tokens instead of introducing a parallel theme.

## Interaction principles

- Server state is authoritative. Missing catalog prices, unavailable APIs, and empty inventories are stated plainly and never replaced with fake production data.
- Navigation uses native links/buttons with keyboard-visible focus and route-local headings.
- Market and inventory filters use intentionally native selects for short OS-owned option lists; forms own their validation copy with `noValidate`.
