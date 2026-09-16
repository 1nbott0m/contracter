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
| 5 | `[x]` VERIFIED (partial scope) — `cef4f0c` | Real multi-connection concurrency: two independently pooled connections racing `post_credit_adjustment` with the same idempotency key (settle once) and with different keys (both apply) | `feature/collection-scarcity-engine` | `crates/db/tests/concurrency.rs` |

Tasks 2–4 were verified offline only (no live PostgreSQL) when first
written. **A local PostgreSQL 17 instance was then installed in this
environment and every `#[ignore]` test in the repository was run for real —
the first time that has happened at any point in this project's history**
(confirmed by the git-history audit: no prior commit or session ever
exercised `TEST_DATABASE_URL`). That run found three genuine bugs no
type-checking could catch, all now fixed and re-verified:

- **`crates/db/tests/inventory.rs`** (commit `ff1a312`): `clock_timestamp()`
  is volatile and can return a different instant on each call within one
  statement; `seed_allocations`' `allocated_at` default and its explicit
  `expires_at` expression were two separate calls that could drift by a few
  microseconds and intermittently trip the 15-second-window `CHECK`. Fixed
  by capturing one clock reading via a CTE for both columns. Reproduced 8/8
  on the old code, 8/8 clean on the fix.
- **`publish_collection_scarcity_snapshot`** (commit `bf76921`, two bugs):
  (a) the Task 3 fix above was architecturally wrong — the Rust wrapper
  INSERTed computed items under the caller's own role, but no role is
  granted `INSERT` on those tables (by design), so it was unusable by any
  application role as merged; moved the whole computation inside the
  `SECURITY DEFINER` function body, matching `post_credit_adjustment` /
  `finalize_contract` / `publish_valuation_snapshot`'s actual shape, which
  also makes the function atomic by construction again (one statement) and
  supersedes the `&mut Transaction` signature from Task 3. (b) a local
  PL/pgSQL variable named `snapshot_id` collided with the identically-named
  table column (`42702` ambiguous column reference) — Postgres only catches
  this at statement execution, not at `CREATE FUNCTION` time, so it was
  invisible until the function actually ran. Renamed to `v_snapshot_id`.
- **`crates/db/tests/scarcity.rs`** fixture bug (same commit): calling
  `insert_active_target` twice in one test created two competing "active"
  stock policy versions instead of one version with two bands; only the
  most-recently-inserted one was ever picked up by the single-active-version
  aggregation query, silently orphaning the first collection's band. Split
  into `insert_active_stock_policy_version` (call once) +
  `insert_stock_policy_band` (call per rarity, same version id).

After all three fixes: `./scripts/verify.sh` passes end-to-end against the
real instance — migrations, seeds, the psql SQL-invariant suite
(`db/tests/001_invariants.sql`, `002_reference_data.sql`), and the full
`cargo test --workspace -- --ignored` run, all green in one pass.

Task 5's scope was narrowed during implementation: a genuine two-user race
for **the last unit of warehouse stock** is not constructible against
current code, and this is itself a finding, not a shortcut — nothing in
this schema ever increments `warehouse_stock.reserved_units` (only
`finalize_contract` decrements it), so the quote-creation reservation flow
that would need to be raced does not exist as callable production code yet,
only as raw fixture SQL in tests. A concurrent `finalize_contract` race test
remains genuinely future work once quote creation exists.

Task 1 still can't be done — it depends on which of
`feature/stock-risk-read-access` or `feature/collection-scarcity-engine`
merges to `main` first, which isn't decided.

## AUDIT COMPLETE — TASKS 2–5 VERIFIED AGAINST A REAL POSTGRESQL INSTANCE, TASK 1 BLOCKED ON MERGE ORDER
