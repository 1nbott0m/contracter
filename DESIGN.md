---
version: alpha
colors:
  canvas: "#080A0F"
  surface: "#10141D"
  surfaceRaised: "#171D28"
  line: "#293242"
  text: "#F6F8FB"
  textMuted: "#9BA7B8"
  cyan: "#57E3F3"
  violet: "#9B7CFF"
  success: "#63E6A7"
  warning: "#FFCA70"
  danger: "#FF7188"
typography:
  display:
    fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(2rem, 4vw, 4.5rem)"
    lineHeight: "0.95"
  body:
    fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1rem"
    lineHeight: "1.5"
  label:
    fontFamily: "Inter, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.6875rem"
    lineHeight: "1.2"
rounded:
  panel: "18px"
  control: "12px"
  pill: "999px"
spacing:
  unit: "4px"
  page: "clamp(18px, 4vw, 64px)"
components:
  panel:
    background: "surface"
    border: "1px solid line"
    radius: "panel"
  primaryButton:
    background: "cyan"
    foreground: "canvas"
    radius: "control"
  rarity:
    role: "semantic accent per item rarity, never the only status signal"
---

## Overview

Contracter is a transparent CC contract platform, not a copy of a casino brand.
The visual north star is **a neon trading terminal staged like a live drop show**:
high-energy moments belong to the contract reveal, while market, inventory and
history remain calm, legible workspaces. Borrow the density and live feedback
patterns of the reference products, never their logos, copy, artwork or betting
claims.

The product register is hybrid: expressive on the public contract stage and
operational on authenticated data screens. The memorable signature is a cyan
``contract beam`` that travels through the selected items into the revealed
result. It must be decorative and reduced-motion safe, never obscure controls.

## Colors

`canvas`, `surface`, `surfaceRaised`, and `line` establish the dark graphite
hierarchy. Cyan is the primary action and live/status signal; violet is the
secondary brand accent. Green, amber and red are semantic only. Rarity colors
may decorate item cards but every rarity also has a text label.

## Typography

Use Inter with system fallbacks. Display text is compact and cinematic; body
copy is ordinary sentence case and readable in Russian. Labels may use uppercase
tracking for navigation and metadata, but instructions and errors must not be
all-caps.

## Layout

Desktop uses a max-width content rail with a sticky top shell, a two-column
contract stage, and four-column catalog grids. Mobile collapses to one column,
keeps a fixed bottom navigation, and preserves document scrolling. No screen
may place the primary action below an unrelated footer.

## Elevation & Depth

Panels use one restrained raised surface and a soft cyan/violet ambient glow.
Avoid stacked shadows and glass blur on every element. Motion and contrast, not
heavy shadows, establish hierarchy.

## Shapes

Panels are rounded 18px; controls 12px; status chips are pills. Cards have a
clear hit area, visible hover/focus/pressed states and stable image slots.

## Components

- `AppShell`: one navigation/header/footer contract across every route.
- `Panel`, `Button`, `StatusChip`, `ToastRegion`: shared visual and behavioral
  owners, not screen-local clones.
- `SkinCard`: fixed media geometry, rarity label, CC value and owned/available
  state.
- `ContractStage`: selection, validating, reveal and result states with a
  reduced-motion path.
- `DataList`: explicit loading, empty, error and cursor-load-more states.
- `Dialog`: app-owned accessible confirmation/reveal details with focus restore.

## Motion & content voice

The reveal timeline is 0.8s gather → 1.2s fuse → 1.6s reveal → result settle;
the user can skip it. All CSS animation honors `prefers-reduced-motion`. Use
plain Russian for actions and errors, with CC as the only internal currency.

## Do's and Don'ts

- Do make the next action obvious and keep the selected-item count visible.
- Do show why an action is unavailable and preserve user input on failures.
- Do use real API states; never show demo inventory in production.
- Don't copy competitor logos, artwork, exact layouts or gambling language.
- Don't use decorative diamonds/lines over item art when they reduce legibility.
- Don't hide important navigation in a footer or rely on hover alone.
