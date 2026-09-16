# Target Architecture & Implementation Plan

## Target architecture: mostly already here

Unlike a greenfield audit, this one found an already-mature schema: single
global warehouse (`inventory_positions` + its structural CHECK), append-only
ledger with a DB-enforced balance invariant, append-only audit journals
enforced by trigger (not convention), idempotency via advisory lock +
unique key on every money/inventory-mutating function, two-person approval
with payload-hash pinning, and role separation (`contracter_runtime` /
`contracter_admin_runtime` / `contracter_readonly`) that structurally denies
any direct journal write even to the narrowest runtime role. No redesign is
warranted — the target architecture *is* the current `main` architecture,
plus the read-access modules already proposed on the four open branches,
plus the five fixes below.

Money/inventory/contract lifecycle, already correctly modeled (see
`docs/superpowers/specs/2026-09-13-database-economy-core-design.md` for the
approved formulas this schema implements):

```
ITEM:      WAREHOUSE ──finalize_contract/market──► USER OWNED
                 ▲                                      │
                 └──────────── returned as contract input┘

CONTRACT:  10 SAME-RARITY INPUTS (locked) ──► quote (commit-reveal) ──►
           finalize_contract (atomic: inputs→warehouse, output→user,
           stock adjust, risk-exposure release, ledger settlement)
```

`crates/db`'s six read-access modules (`catalog`, `inventory`, `ledger`,
`pricing`, and — pending merge — `stock`, `quotes`, `contracts`, `scarcity`)
map 1:1 onto this schema with no gaps: every table that needs a
runtime-readable projection has one, and none expose a table the runtime
role isn't already granted.

## Implementation plan (P1 items only — P2/P3 are optional cleanup, not blocking)

Each task is scoped to its own open branch, stays inside DATABASE ONLY, adds
no new business logic, and gets `RED → GREEN → VERIFY` per CLAUDE.md.

| # | Task | Branch | Files | Test to add first |
|---|---|---|---|---|
| 1 | Rename `0009_stock_policy_read_grants.sql` → `0010_*` **or** rename `0009_collection_scarcity.sql` → `0010_*`, whichever branch merges second | whichever merges second | one filename | `crates/db/tests/postgres.rs` migration-count assertion already catches a missing/miscounted migration |
| 2 | `apply_collection_scarcity`: validate all input `WeightedOutcome`s share one `weight_denominator`; return `TradeupError::InvalidWeights` on mismatch | `feature/collection-scarcity-engine` | `crates/economy-core/src/tradeup.rs` | new test: build two outcome sets with different denominators, assert rejection |
| 3 | `publish_collection_scarcity_snapshot`: take `&mut Transaction<'_, Postgres>` instead of `&mut PgConnection`, or wrap its three statements in an internal transaction | `feature/collection-scarcity-engine` | `crates/db/src/scarcity.rs` (+ call-site update in `crates/db/tests/scarcity.rs`) | existing tests continue to pass; add one test that forces the second statement to fail (e.g. FK violation) and asserts no snapshot row survives |
| 4 | New migration on the same branch: add `collection_scarcity_snapshots`/`collection_scarcity_snapshot_items` to 0005's append-only-guard arrays; `GRANT EXECUTE ON FUNCTION publish_collection_scarcity_snapshot(bigint) TO contracter_admin_runtime` | `feature/collection-scarcity-engine` | new `00XX_collection_scarcity_guards.sql` | extend `crates/db/tests/scarcity.rs`'s runtime-role test to also assert `contracter_admin_runtime` (when it exists) can `EXECUTE` the function, and that direct `INSERT` as `contracter_runtime` is rejected |
| 5 | Add real multi-connection concurrency tests: at minimum, two simultaneous `finalize_contract` calls racing for the same single-unit `warehouse_stock` row, and two simultaneous `post_credit_adjustment` calls with the same idempotency key from different connections | new branch, `main`-based (exercises already-merged functions) | new `crates/db/tests/concurrency.rs`, using two independent `PgPool`/`Database` connections and `tokio::join!` | this task *is* the test — RED is "no such test exists today" |

Tasks 1–4 are small, mechanical, and confined to code I already wrote on an
unmerged branch — no product decision involved. Task 5 is the one genuinely
open-ended item: it requires a live `TEST_DATABASE_URL`, which has not been
available in this environment at any point this session (every `#[ignore]`
integration test across every branch has been compiled but never executed
here). I flag this rather than guess at results.

## AUDIT COMPLETE — WAITING FOR IMPLEMENTATION APPROVAL
