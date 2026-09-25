# CONTRACTER complete frontend — unified design

## Goal

Deliver a real multi-route frontend application that combines the master prompt's
production requirements with the user's cinematic contract reference. The
experience remains a contract workflow, never a case-opening or gambling flow.

## Product flow

`SELECT → REVIEW → LOCK → SIGN → MERGE → RESULT`.

The reveal uses four to ten contract display frames inspired by the reference
video: graphite surfaces, controlled cyan/violet/amber edge lighting, real
canonical skin artwork, and a restrained reflective depth treatment. Frames are
not cases or crates, do not contain random near-miss mechanics, and do not imply
odds or jackpots. The server-provided result is shown only after the commit
request succeeds.

## Application structure

Shared `AppShell`, `Logo`, `LiveActivity`, `Header`, `GlobalSearch`, `ProfileMenu`,
`SkinImage`, `SkinCard`, `ContractBuilder`, `ContractSummary`, `MarketGrid`,
`InventoryGrid`, `HistoryList`, `ContractDetails`, `ContractReveal`, `Footer`.

Routes: `/contracts`, `/market`, `/inventory`, `/history`, `/contracts/:id`,
`/profile`, `/login`, `/transparency`, `/terms`, `/privacy`, `/support`, and a
real 404 state. Existing backend API remains the source of truth; development
fallback data is isolated and visibly non-production.

## Data and interaction rules

- One canonical skin definition resolves artwork everywhere.
- Every async view has loading, success, empty, error, and retry states.
- Live activity and online count are real backend data or explicitly unavailable;
  never fabricated as production facts.
- Search, filters, sorting, navigation, profile actions, market purchase,
  inventory selection, history details, verification, and footer links are real.
- No confidential economic or cryptographic internals are rendered.

## Visual and accessibility rules

Precision Graphite dominates. Real skin artwork supplies color. Motion is staged,
slower, and purposeful: case-like display frames hold long enough to inspect,
then converge into the signing core and reveal the result. Respect keyboard
focus, Escape, semantic controls, reduced motion, reserved image dimensions, and
mobile layouts at 375/390/430/768/1024/1440px.

## Validation

Run TypeScript/build checks, route and interaction smoke checks, image/error
checks, mobile viewport checks, and a production deployment verification against
the actual public URL. Do not claim completion while any visible control is dead
or any required route is missing.
