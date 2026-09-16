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

| # | Status | Task | Branch | Files |
|---|---|---|---|---|
| 1 | `[ ]` NOT STARTED — blocked on merge order | Rename `0009_stock_policy_read_grants.sql` → `0010_*` **or** rename `0009_collection_scarcity.sql` → `0010_*`, whichever branch merges second | whichever merges second | one filename |
| 2 | `[x]` VERIFIED — `683dad4` | `apply_collection_scarcity`: validate all input `WeightedOutcome`s share one `weight_denominator` | `feature/collection-scarcity-engine` | `crates/economy-core/src/tradeup.rs`, `tests/tradeup.rs` (new `mismatched_input_denominators_are_rejected`, RED confirmed before the fix) |
| 3 | `[x]` VERIFIED — `3e5ec1a` | `publish_collection_scarcity_snapshot` now takes `&mut Transaction<'_, Postgres>` instead of `&mut PgConnection` — atomicity by construction, not caller discipline | `feature/collection-scarcity-engine` | `crates/db/src/scarcity.rs`, `tests/scarcity.rs` (4 call sites updated) |
| 4 | `[x]` VERIFIED — `2de6186` | New migration: `collection_scarcity_snapshot_items` added to 0005's append-only-guard pattern (matching `valuation_snapshot_items` exactly — the header table and the `current_*` projection are deliberately left unguarded, mirroring `valuation_snapshots`/`current_valuations`); `GRANT EXECUTE` on the publish function to `contracter_admin_runtime` | `feature/collection-scarcity-engine` | `db/migrations/0010_collection_scarcity_guards.sql`, `tests/scarcity.rs` (+2 tests: admin-runtime can execute; runtime role's direct `INSERT` is rejected `42501`), `tests/postgres.rs` (migration count → 10) |
| 5 | `[ ]` NOT STARTED — needs a live PostgreSQL | Real multi-connection concurrency tests: two simultaneous `finalize_contract` calls racing for the same single-unit `warehouse_stock` row; two simultaneous `post_credit_adjustment` calls with the same idempotency key from different connections | new branch, `main`-based | new `crates/db/tests/concurrency.rs` |

Tasks 2–4: `cargo fmt --check` / `clippy -D warnings` / `cargo test
--workspace` / `./scripts/verify.sh` / `git diff --check` all pass after
each commit (offline suite only — no `TEST_DATABASE_URL` in this
environment). Task 1 can't be done yet — it depends on which of
`feature/stock-risk-read-access` or `feature/collection-scarcity-engine`
merges to `main` first, which isn't decided. Task 5 needs a real PostgreSQL
instance, which has not existed in this environment at any point this
session.

## AUDIT COMPLETE — TASKS 2-4 VERIFIED, TASKS 1 AND 5 BLOCKED
