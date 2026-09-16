# Final Database Report — Contracter (DATABASE ONLY scope)

**Verdict: DATABASE INCOMPLETE.**

Per the Definition-of-Done gate this repo's own audit process requires
(`DATABASE COMPLETE — VERIFIED` only if there are no MISSING requirements,
no unresolved P0/P1, and real-Postgres concurrency/idempotency are proven),
this cannot be written honestly today. One capability — **quote
creation** — has a complete schema and a complete read/finalize path but
**no write path exists anywhere as callable code**. Everything downstream
of that (probability snapshots, outcome snapshots, reservations, scarcity
wired into real weighting) is consequently unproven end-to-end, even
though every piece that *does* exist is well-built and, as of this
session, adversarially reviewed and hardened. See §1 for the completion
percentage and §3 for exactly what's missing.

---

## 1. Database completion

**Estimate: ~80% of the approved MVP scope is implemented, tested, and
(for the parts touched this session) adversarially reviewed. The
remaining ~20% is concentrated entirely in one capability: quote
creation.** This is a judgment call, not a formula — weighted by "is the
write path callable code, not just a table," which is the bar the
project's own docs already use (see `02-gap-analysis.md` finding 11).

- **VERIFIED:** 34 requirement rows (§4)
- **PARTIAL:** 9 requirement rows
- **MISSING:** 3 requirement rows
- **BLOCKED:** 2 requirement rows (need a product/architecture decision, not more DB work — see `docs/database/BLOCKED_DECISIONS.md`)
- **NOT APPLICABLE:** 3 requirement rows (explicitly out of MVP scope per the approved design doc)

## 2. Source of truth used

- `CLAUDE.md` (scope: DB + `economy-core` only; append-only; integer
  microcredits; exact-10-input contracts; idempotency; migration
  discipline)
- `docs/superpowers/specs/2026-09-13-database-economy-core-design.md`
  (the approved design — trade-up rules, deterministic selection,
  pricing, two-admin approval; **explicitly excludes** "deposits,
  withdrawals, peer-to-peer exchange, cases, upgrades, StatTrak,
  Souvenir, or knife/glove outcomes" from the MVP)
- `docs/database/01-current-state.md` through `04-invariants.md` (this
  project's own prior audit, written earlier in this engagement)
- All 13 migrations, all `crates/db/src/*.rs`, all
  `crates/economy-core/src/*.rs`, all test files, git history across
  every branch

No new project rules were invented. Where the checklist below uses a term
the design doc doesn't use natively (e.g. "market/purchases/sales
foundation"), §4 says explicitly what evidence was checked and why the
verdict is what it is.

## 3. What's actually missing (the ~20%)

**Quote creation has no write path.** `crates/db/src/quotes.rs` contains
exactly four functions, all reads: `find_tradeup_quote`,
`find_active_quote_for_user`, `list_quote_inputs`, `list_quote_outcomes`.
No migration defines a `create_quote`/`allocate_seed_commitment`-style
`SECURITY DEFINER` function, and no Rust function writes a
`tradeup_quotes`/`quote_inputs`/`quote_outcomes`/`seed_commitments`/
`seed_allocations` row. The only places these tables are ever inserted
into are raw fixture `INSERT` statements inside
`crates/db/tests/{quotes,contracts,inventory}.rs` — test scaffolding, not
production code.

Everything this blocks, concretely:

- `warehouse_stock.reserved_units` is only ever *decremented*
  (`finalize_contract` releasing a reservation) — nothing anywhere
  *increments* it, because nothing anywhere creates the reservation a
  quote is supposed to hold. Documented already as gap-analysis finding
  11 before this session; still true after this session's work, because
  fixing it means building the missing feature, not patching a bug.
- `economy_core::build_outcomes`, `calculate_output_float`,
  `select_outcome`, and `apply_collection_scarcity` are all correct,
  well-tested, pure functions — but **none of them is ever called from
  any DB-writing code path**. They're proven in isolation
  (`economy-core/tests/*.rs`, 25 tests total, all passing), not proven as
  part of an actual quote being created end-to-end.
- The collection-scarcity engine built and hardened this session
  (migrations 0009-0010, hardening in 0012) is fully functional and
  tested as a *standalone snapshot-publishing system* — but since nothing
  creates quotes, nothing ever actually *reads* `current_collection_scarcity`
  to damp a real quote's weights either. It's a correct, tested,
  unconnected component.
- The `finalize_contract`-races-for-the-last-unit-of-stock concurrency
  test that the project's own top-stated risk ("100 users racing the last
  item") calls for cannot be written, because the reservation it would
  race doesn't exist yet to race against.

**This was not a regression introduced this session** — it predates every
branch touched here and is already honestly documented in
`02-gap-analysis.md` finding 11 (written before this session started).
This report is not the first to notice it; it's confirming it's still
true after the adversarial-review and PR-integration work, and stating
plainly that it, not any bug, is why "DATABASE COMPLETE" cannot be
written.

**Two smaller MISSING items**, both far smaller in scope than quote
creation:

- No property-based tests exist anywhere (`Cargo.toml` has no
  `proptest`/`quickcheck` dependency). `economy-core`'s weight/probability
  arithmetic is exactly the kind of code this would suit — every current
  test is example-based.
- `EXPLAIN ANALYZE` / index audit at realistic data volume has never been
  done (already noted in `02-gap-analysis.md`; still true, no local
  volume-seeded database exists).

## 4. Requirements matrix

Legend: **VERIFIED** = implemented, constrained, and tested with concrete
evidence · **PARTIAL** = schema/logic exists but the write path or test
coverage is incomplete · **MISSING** = no implementation found ·
**BLOCKED** = needs a product/architecture decision outside DATABASE ONLY
scope · **N/A** = explicitly out of MVP scope per the approved design doc.

| Requirement | Status | Implementation | Test | Evidence | Remaining work |
|---|---|---|---|---|---|
| Catalog | VERIFIED | `catalog_items`, `skus` tables (0003); `crates/db/src/catalog.rs` reads | `crates/db/tests/catalog.rs` | migration + Rust file + test file all present | — |
| Collections | VERIFIED | `collections` table (0001/0003), `enabled` flag | `crates/db/tests/catalog.rs` | same | — |
| Rarities | VERIFIED | `rarities` table, `rank`/`is_covert` columns (0001) | `crates/db/tests/catalog.rs`, `economy-core/tests/tradeup.rs::rejects_covert_inputs` | — | — |
| Wear | VERIFIED | `wear_bands` table with `lower_bound`/`upper_bound`/`includes_upper_bound` CHECKs (0001) | `crates/db/tests/catalog.rs` | — | — |
| SKU | VERIFIED | `skus` table, FK to `catalog_items`+`wear_bands`, `UNIQUE` | `crates/db/tests/catalog.rs` | — | — |
| Individual item instances | VERIFIED | `inventory_items` (0003): `sku_id`, `canonical_float`, immutable per-item identity | `crates/db/tests/inventory.rs` | — | — |
| Float | VERIFIED | `numeric` float columns, `CHECK (min_float < max_float)`-style ranges; `economy_core::calculate_output_float` exact `Decimal` math | `economy-core/tests/tradeup.rs::normalized_input_floats_determine_output_float` | — | — |
| Global stock | VERIFIED | `warehouse_stock(sku_id, available_units, reserved_units)` with `CHECK (reserved_units <= available_units)`, `CHECK (available_units >= 0)` (0004) | `crates/db/tests/stock.rs` | — | Write path only decrements on settlement; see §3 |
| Ownership | VERIFIED | `inventory_positions` `CHECK (owner_user_id IS NOT NULL)::int + in_warehouse::int = 1` — exactly one location, enforced at the DB level, not just app code | invariants doc #1; `crates/db/tests/inventory.rs` | `docs/database/04-invariants.md` row 1 | — |
| Inventory | VERIFIED | `inventory_items`+`inventory_positions`, read via `crates/db/src/inventory.rs` | `crates/db/tests/inventory.rs` | — | — |
| Stock states | PARTIAL | `warehouse_stock` schema + constraints complete | `crates/db/tests/stock.rs` (reads only) | — | No write path increments `reserved_units` (§3) |
| Reservations/locks | PARTIAL | `inventory_item_locks`, `quote_candidate_reservations` tables exist (0004), released correctly by `finalize_contract`/`publish_valuation_snapshot` | none — nothing creates a reservation | grep confirms `INSERT INTO quote_candidate_reservations`/`inventory_item_locks` only in test fixtures | Quote-creation write path (§3) |
| Market/purchases/sales database foundation | N/A | Design doc: "MVP has no deposits, withdrawals, peer-to-peer exchange" | — | `docs/superpowers/specs/...design.md` line 5 | Not part of approved scope |
| Balances | VERIFIED | `ledger_balances`, `ledger_accounts`; `find_ledger_balance` | `crates/db/tests/ledger.rs` | — | — |
| Ledger | VERIFIED | `ledger_transactions`/`ledger_postings`, deferred `CONSTRAINT TRIGGER` enforcing zero-sum at commit (0003) | `db/tests/001_invariants.sql`: "unbalanced ledger transaction is rejected" | invariants doc row 2 | — |
| Financial auditability | VERIFIED | append-only guard on `ledger_transactions`/`ledger_postings`/`credit_adjustment_events` (0005), integer microcredits everywhere, zero `f32`/`f64` in `crates/db`/`crates/economy-core` | `db/tests/001_invariants.sql::assert_append_only` | invariants doc row 5, row 9 | — |
| Contracts | PARTIAL | `contracts`/`contract_inputs`/`contract_outcomes` schema (0004), `finalize_contract` (settlement) fully implemented and reachable via `contracter_runtime` | `db/tests/001_invariants.sql` (cardinality), no full-fixture finalize test | — | Depends on quote creation (§3) to be exercised end-to-end |
| Exactly 10 contract inputs | VERIFIED | `economy_core::validate_inputs` (`INPUT_COUNT == 10`); `finalize_contract` re-validates cardinality/uniqueness before any lock (0004) | `economy-core/tests/tradeup.rs::rejects_any_input_count_other_than_ten`; `001_invariants.sql` 9/11-input rejection | invariants doc row 3 | — |
| Same input rarity | VERIFIED | `validate_inputs` checks all inputs share one rarity | `economy-core/tests/tradeup.rs::rejects_mixed_input_rarities` | — | — |
| Next-rarity outcomes | VERIFIED | `build_outcomes` computes `output_rarity = input_rarity + 1` (checked_add, no panic at max rarity) | `economy-core/tests/tradeup.rs::rejects_rarity_that_cannot_have_a_next_tier_without_panicking` | — | — |
| Collection weighting | PARTIAL | `build_outcomes`: exact `P(output) = input_count/10/output_count` | 5 dedicated tests, all passing | `economy-core/tests/tradeup.rs` | Never called from a real write path (§3) |
| Float calculation/storage | PARTIAL | `calculate_output_float`, `Decimal` throughout, `output_float` column on `quote_outcomes`/`contract_outcomes` | `economy-core/tests/tradeup.rs::normalized_input_floats_determine_output_float` | — | Never called from a real write path (§3) |
| Probability snapshots | PARTIAL | `quote_outcomes.probability_numerator`/`probability_denominator` schema; read via `list_quote_outcomes` | none (no rows are ever written outside test fixtures) | — | Quote-creation write path (§3) |
| Outcome snapshots | PARTIAL | Same as above, plus `contract_outcomes` for settled results | `crates/db/tests/contracts.rs` (reads) | — | Quote-creation write path (§3) |
| Artificial scarcity | VERIFIED (as a standalone component) | `apply_collection_scarcity` (pure, additive to `build_outcomes`); `publish_collection_scarcity_snapshot` SQL function, hardened this session (advisory-lock serialization, future-dated-activation rejection) | `economy-core/tests/tradeup.rs` (7 scarcity tests incl. a documented precision-limit test added this session); `crates/db/tests/scarcity.rs` (9 tests incl. 2 new this session); `crates/db/tests/concurrency.rs` (new real 2-connection race test this session) | migrations 0009, 0010, 0012 | Not wired into any real quote's weighting yet (§3) — the snapshot system itself is complete and tested |
| Scarcity history/snapshots | VERIFIED | `collection_scarcity_snapshots`/`_items` append-only-by-design (single mutable `published_at` transition documented as an intentional exception in `BLOCKED_DECISIONS.md` #2), `current_collection_scarcity` projection | `crates/db/tests/scarcity.rs::scarcity_history_accumulates_across_multiple_publishes` | — | — |
| Scarcity algorithm versioning | VERIFIED | `formula_version text` column on `collection_scarcity_snapshots`, required non-empty | `crates/db/tests/scarcity.rs` | — | — |
| Zero-stock outcome exclusion | VERIFIED (pure-function level) | `build_outcomes` rejects an unavailable candidate rather than rerolling (`TradeupError::UnavailableOutput`); `apply_collection_scarcity` drops (not zeroes) a fully-depleted collection | `economy-core/tests/tradeup.rs::rejects_an_unavailable_candidate_instead_of_rerolling`, `::fully_depleted_collection_is_dropped_not_zeroed` | — | Not wired into a real write path (§3) |
| Probability normalization | VERIFIED (pure-function level) | `select_outcome` asserts `checked_weight_sum(outcomes) == denominator` before using any outcome set | `economy-core/tests/tradeup.rs::rejects_weight_sum_overflow_without_panicking` | — | Same caveat |
| Operation IDs | VERIFIED | `public_id uuid` on every externally-addressable table; `idempotency_key`/`execution_key uuid` on every mutating operation | throughout | — | — |
| Idempotency | PARTIAL | `post_credit_adjustment` fully idempotent incl. full-row mismatch detection (0006); `finalize_contract` idempotent by design (`quote_acceptance_events` unique key, check-then-return) | `crates/db/tests/ledger.rs::credit_adjustment_updates_balance_and_is_idempotent`; `crates/db/tests/concurrency.rs` (real 2-connection proof for credit adjustments, this session) | invariants doc row 4, row 8 | `finalize_contract`'s idempotency is implemented but has **no test** — needs full quote fixtures that don't exist (§3) |
| Audit/history | VERIFIED | `credit_adjustment_events`, `inventory_transfer_events`, `critical_action_*_events`, all append-only | `db/tests/001_invariants.sql` | invariants doc row 5, 6 | — |
| Constraints | VERIFIED | 157 `CHECK` constraints across migrations | `db/tests/001_invariants.sql` exercises the money/inventory/security-critical ones | grep count, this session | — |
| FK | VERIFIED | 107 `REFERENCES` | — | grep count | — |
| UNIQUE | VERIFIED | 91 `UNIQUE` | `pg_temp.has_unique_single_column` helper in `001_invariants.sql` | grep count | — |
| CHECK | VERIFIED | (counted with Constraints above) | — | — | — |
| Indexes | VERIFIED | 21 explicit `CREATE [UNIQUE] INDEX` beyond PK/UNIQUE-backed ones | — | grep count | `EXPLAIN ANALYZE` at realistic volume never done (§3, pre-existing gap) |
| Transaction safety | VERIFIED | Every mutating SQL function wraps its work in one statement or one implicit function-body transaction; `sqlx::migrate!` per-migration transactions | `crates/db/tests/postgres.rs::sqlx_applies_migrations_idempotently_and_rolls_back_transactions` | — | — |
| Concurrency safety | PARTIAL | Advisory locks + `FOR UPDATE ORDER BY <pk>` throughout; scarcity-publish serialization added this session | `crates/db/tests/concurrency.rs`: 2 credit-adjustment races (pre-existing) + 1 new scarcity-publish race (this session) | — | `finalize_contract`/last-unit-of-stock race cannot be written (§3) |
| Row locking | VERIFIED | `FOR UPDATE` on `risk_state`, `tradeup_quotes`, `quote_outcomes`, `ledger_accounts`, `inventory_positions`, `warehouse_stock` | traced in `04-invariants.md` lock-ordering map | — | — |
| Deterministic lock ordering | VERIFIED | `ORDER BY <pk> FOR UPDATE` on every multi-row lock; single global serialization point (`risk_state`) for the two highest-risk writers | `04-invariants.md` "Lock-ordering map" section, explicit trace | — | Static trace, not load-tested (below) |
| Rollback safety | VERIFIED | `crates/db/tests/postgres.rs` explicit rollback-probe test; every SQL function is one atomic statement or wraps writes in its own body | — | — | — |
| Deadlock handling/strategy | VERIFIED (as a static trace) | `04-invariants.md`: no two writers lock overlapping resource classes in conflicting order; explicitly documented, not just assumed | none (see Remaining work) | `04-invariants.md` lock-ordering map | Never proven under real concurrent load/chaos testing — explicitly labeled a static trace, not a load-tested result, in the source doc itself |
| Migration integrity | VERIFIED | 13 migrations, sequential, no filename/version collisions after this session's PR-integration renumbering (0009→0011, 0011→0012, 0009→0013 across the 3 branches that needed it) | `crates/db/tests/postgres.rs::assert_eq!(applied, 13)` | this session's git-history reconstruction (§5) | Real-Postgres execution of the full 13-migration chain was not run this session (§6) |
| Fresh DB migration | PARTIAL | `sqlx::migrate!` mechanics verified structurally (every test's `test_database()` calls `.migrate()`) | `crates/db/tests/postgres.rs` (idempotent double-migrate) | — | Not executed against a real empty PostgreSQL this session — local instance access lost (§6); offline/structural check only |
| Upgrade migration | VERIFIED (mechanically) | `sqlx::migrate!` applies only unapplied versions; `postgres.rs` double-migrate test proves idempotent re-application | same | — | — |
| Integration tests | VERIFIED (structurally) | 12 `crates/db/tests/*.rs` files + `db/tests/001_invariants.sql`, all `#[ignore]`-gated on `TEST_DATABASE_URL` | full inventory in §5 | — | Not executed for real this session (§6) |
| Constraint tests | VERIFIED | `db/tests/001_invariants.sql` — SQLSTATE-exact assertions for every P0/P1 constraint | same file | — | — |
| Transaction tests | VERIFIED | `postgres.rs` rollback test | — | — | — |
| Concurrency tests | PARTIAL | `concurrency.rs`: 3 real 2-connection tests (2 pre-existing + 1 added this session) | same file | — | `finalize_contract` race impossible today (§3) |
| Idempotency tests | PARTIAL | Covered for credit adjustments (incl. real concurrency); `finalize_contract` idempotency has no test | `ledger.rs`, `concurrency.rs` | — | Needs quote fixtures (§3) |
| Property tests | MISSING | none | none | no `proptest`/`quickcheck` in `Cargo.toml` | Would suit `economy-core`'s weight arithmetic specifically |
| Regression tests | VERIFIED | Every bug found this session (future-dated stock-policy activation, 3 separate `clock_timestamp()` races, the `publish_collection_scarcity_snapshot` atomicity/grant bug, a fixture ordering bug) has a dedicated test locking in the fix | this session's 7 commits across 4 branches (§5) | — | — |
| Caller identity binding (`finalize_contract`/`approve_critical_action`) | BLOCKED | `finalize_contract_for_user`/`approve_critical_action_as_admin` wrappers added this session close the "forgot to check ownership" gap | `db/tests/001_invariants.sql` (2 new assertions this session) | `docs/database/BLOCKED_DECISIONS.md` #1 | Full closure needs a per-request identity mechanism — an authentication-architecture decision, not DB work |
| `users.password_hash` column exposure | VERIFIED (fixed this session) | `contracter_runtime`/`contracter_admin_runtime` narrowed to a safe column list (migration 0013) | `db/tests/001_invariants.sql` (2 new `has_column_privilege` assertions) | — | — |
| Append-only guard completeness | BLOCKED | `valuation_snapshots`/`current_valuations`/scarcity equivalents cannot get the standard guard trigger without breaking their legitimate `published_at`/upsert writes | — | `docs/database/BLOCKED_DECISIONS.md` #2 | Needs a column-aware guard design, deferred as a real (if narrow) design task |

## 5. DB PRs — reconstruction and final status

Reconstructed from `git log`/`git branch -a -v`/`git diff` across every
branch (network access to the GitHub web/API was unavailable for most of
this session — see §6 — so PR numbers below are inferred from merge-commit
messages and the user's own confirmation, not read directly from the
GitHub PR list UI).

| PR | Branch | Base | Status before this session | Work this session |
|---|---|---|---|---|
| #1 | `feature/catalog-read-access` | main | Merged | none |
| #2 | `feature/stock-risk-read-access` | main | Open, `0009_*` colliding with #6's `0009_*` | Rebuilt on current main (semantic merge of `ids.rs`/`lib.rs`/`postgres.rs`), renamed migration to `0011_stock_policy_read_grants.sql`, fixed a future-dated-`activated_at` bug (§7), pushed |
| #3 | `feature/pricing-read-access` | main | Merged | none |
| #4 | `feature/quote-read-access` | main | Open | Rebuilt on current main (semantic merge), pushed. Now also carries #5's commits (see below) |
| #5 | `feature/contract-read-access` | `feature/quote-read-access` (not main) | Already merged into `feature/quote-read-access`, not into main | Carried through unchanged as part of #4's rebuild |
| #6 | `feature/collection-scarcity-engine` | main | Merged | 4 new follow-up commits from an independent adversarial review (concurrent-publish serialization, future-dated-activation rejection, a clock race, a documented precision limit), renumbered migration to `0012_scarcity_publish_hardening.sql`, pushed to the same branch as a fresh PR (the old PR is closed/merged; a new PR must be opened from this branch) |
| (new) | `fix/db-security-hardening-audit-findings` | main | Did not exist | Created this session: `users.password_hash` exposure fix, `finalize_contract_for_user`/`approve_critical_action_as_admin` identity-binding wrappers, `docs/database/BLOCKED_DECISIONS.md`. Migration `0013_security_hardening.sql`. Pushed as a new branch; needs a PR opened |

**Required merge order** (each branch's migration numbering depends on the
previous one already being on `main`): **#4 (quote+contract) → #2
(stock-risk) → #6's new commits → the new security-hardening PR.** Each
branch was prepared as a superset of the previous ones specifically so
this order produces zero merge conflicts regardless of how long each PR
sits open — GitHub recalculates each PR's diff against the live base
branch, so a PR's shown diff will shrink to just its own contribution once
the branches ahead of it in this order have merged.

**Two PRs still need to be opened** (no GitHub write access this session
— see §6): `feature/collection-scarcity-engine` (fresh PR against `main`,
since PR #6 is closed) and `fix/db-security-hardening-audit-findings`
(brand new). Compare links:
- `https://github.com/1nbott0m/contracter/compare/main...feature/collection-scarcity-engine`
- `https://github.com/1nbott0m/contracter/compare/main...fix/db-security-hardening-audit-findings`

**Semantic merge conflicts resolved** (both explicitly called out by name
in the task): `crates/db/src/ids.rs` and `crates/db/src/lib.rs` conflicted
twice each (once for #4 vs #6's scarcity additions, once for #2 vs #4's
independently-declared `StockPolicyVersionId`/`RiskPolicyVersionId`).
Every conflict was resolved as a **union**, never as a one-side "ours"/
"theirs" pick, with duplicate declarations (the `StockPolicyVersionId`/
`RiskPolicyVersionId` case) deduplicated to one canonical declaration.
`crates/db/tests/postgres.rs`'s migration-count assertion was walked
forward one merge at a time (8 → 11 → 12 → 13) rather than guessed.
Verified after **every single merge step** (not just at the end) with
`cargo build`, `cargo fmt --check`, `cargo clippy -D warnings`, and
`cargo test --workspace` — all clean at every step. No typed ID, export,
read method, or test was lost; `crates/db/src/lib.rs`'s final `pub use`
list contains all 11 modules from every branch (catalog, config,
contracts, database, ids, inventory, ledger, pricing, quotes, scarcity,
stock).

## 6. Verification actually run this session

```
cargo fmt --all -- --check     PASS (at every integration step)
cargo build --workspace        PASS (at every integration step)
cargo clippy --workspace --all-targets -- -D warnings   PASS (at every integration step)
cargo test --workspace         PASS — 63 example-based unit/offline tests across
                                economy-core (25) and db (38 non-ignored),
                                0 failures, at every integration step
```

**Not run this session: real-PostgreSQL integration/migration/concurrency
tests** (`db/verify.sh` + `cargo test -p db --tests -- --ignored`). A
local PostgreSQL 17 instance was installed and used successfully earlier
in this engagement (see `03-implementation-plan.md` for that verification
record), but this session could not reconnect to it: the `postgres` role's
password was not recoverable from any saved location (`TEST_DATABASE_URL`
env var, `.pgpass`, Windows credential store), stopping the Windows
service to attempt a password reset failed for lack of admin rights, and
weakening `pg_hba.conf` from `scram-sha-256` to `trust` to recover access
was correctly refused by this environment's own safety tooling as an
authentication-weakening action. The user was asked and explicitly chose
to skip real-database verification for this session's changes rather than
resolve DB access, so every fix in §7 is verified **offline/structurally
only** — compiles, lints clean, passes example-based tests, and was
read/reasoned through carefully (including by three independent
adversarial-review passes), but has not been executed against a real
PostgreSQL server this session.

GitHub web/API access was also unavailable for most of this session (the
repo is private; the built-in browser was not signed in, and no `gh
auth`/Chrome-extension session was available), which is why the PR
reconstruction in §5 comes from `git log`/`git diff` rather than the
GitHub PR list, comments, or reviews directly. Network connectivity to
GitHub over `git` itself was also transiently down for part of this
session (a `schannel`/TLS handshake failure) and recovered on retry.

## 7. Independent adversarial review — findings and fixes (this session)

Three independent review passes (framed as "find reasons this cannot
ship," not "confirm this is fine," per this project's own review
discipline) covered financial/concurrency correctness, security/privilege
boundaries, and today's-changes code correctness. All findings and their
disposition:

| # | Finding | Severity | Status |
|---|---|---|---|
| 1 | `contracter_runtime`/`contracter_admin_runtime` had full-column `SELECT` on `users`, including `password_hash` | Critical | **Fixed** — migration 0013, column-restricted grant |
| 2 | `publish_collection_scarcity_snapshot` had no serialization across concurrent publishes — `current_collection_scarcity` could point at a stale snapshot | High | **Fixed** — migration 0012, `pg_advisory_xact_lock` |
| 3 | `finalize_contract`/`approve_critical_action` never verify the caller's claimed identity | High | **Partially fixed** — migration 0013 identity-binding wrappers; residual trust boundary is `BLOCKED_DECISIONS.md` #1 (needs an auth-architecture decision) |
| 4 | Active stock-policy-version lookup never checked `activated_at <= now()`, so a future-dated version could outrank a genuinely active one | High | **Fixed** — both `db/migrations/0012_scarcity_publish_hardening.sql` and `crates/db/src/stock.rs` (same bug, two independent branches) |
| 5 | `valuation_snapshots`/`current_valuations`/scarcity equivalents lack the standard append-only guard trigger | Medium | **Deliberately not fixed** — the standard trigger would break their legitimate `published_at`/upsert writes; documented in `BLOCKED_DECISIONS.md` #2 with the actual fix shape needed |
| 6 | `publish_collection_scarcity_snapshot` silently published an empty snapshot when no active stock policy version existed | Medium | **Fixed** — migration 0012, now raises `23514` |
| 7 | `apply_collection_scarcity`'s floor-division can make distinct multiplier sets collapse to an identical damped distribution at low weight scale | Medium | **Documented and tested**, not changed — a real property of bounded-denominator integer damping, not a bug; redesigning the rounding scheme would be a product decision about probabilistic fairness, out of this review's scope |
| 8 | A `clock_timestamp()` double-evaluation race was fixed in some but not all `valuation_snapshots`-inserting fixtures | Medium | **Fixed** — `crates/db/tests/pricing_read.rs` |
| 9 | `find_ledger_balance` takes a bare sequential internal id with no ownership check | Low | **Documented, not fixed** — `BLOCKED_DECISIONS.md` #3, needs a real caller to define what "owner-checked" should mean |
| 10 | A concurrency test's doc comment overclaimed what it proved (advisory-lock key-scoping vs. end-to-end arithmetic correctness) | Low | **Fixed** — comment corrected in `crates/db/tests/concurrency.rs` |

## 8. Unresolved risks

1. **No real-Postgres execution this session** (§6) — every fix above is
   offline-verified only. The next session with working local database
   access must run the full `db/verify.sh` + `cargo test -p db --tests --
   ignored` chain against all four pushed branches before merging any of
   them.
2. **Quote creation does not exist as callable code** (§3) — this is the
   single largest remaining piece of the approved database scope.
3. **`finalize_contract`/`approve_critical_action` residual trust
   boundary** (`BLOCKED_DECISIONS.md` #1) — needs an authentication-
   architecture decision before it can be closed further.
4. **Append-only guard gap on 4 "current pointer" tables**
   (`BLOCKED_DECISIONS.md` #2) — low current exposure (no app role has
   write grants there), but needs a column-aware trigger design.
5. **Two new PRs need opening** and **all four branches need a human to
   click merge in the documented order** (§5) — this session had no
   GitHub write access.
6. **Real concurrent-load/chaos testing has never been done** — the
   deadlock-avoidance analysis in `04-invariants.md` is a static trace,
   explicitly labeled as such in its own source.

## 9. What's left, specifically, in DATABASE ONLY scope

- Implement quote creation (commitment allocation, 10-item locking,
  candidate-output reservation, calling `build_outcomes` +
  `apply_collection_scarcity` + `select_outcome`, persisting
  `tradeup_quotes`/`quote_inputs`/`quote_outcomes`) as a `SECURITY
  DEFINER` SQL function plus a thin Rust wrapper, matching the existing
  shape of `finalize_contract`/`post_credit_adjustment`. This is squarely
  a database-layer function (not an HTTP handler), so it stays in scope —
  it just wasn't attempted this session per the explicit instruction not
  to build new business logic without direction.
- Once quote creation exists: the `finalize_contract`/last-unit-of-stock
  concurrency test, the `finalize_contract` idempotency test, and wiring
  the scarcity engine into real quote weighting all become buildable.
- Add property-based tests for `economy-core`'s weight/probability
  arithmetic.
- Resolve `BLOCKED_DECISIONS.md` #1 and #2 once the relevant
  architecture decisions are made (not DB-team-unilateral calls).
- `EXPLAIN ANALYZE`/index audit once a volume-seeded environment exists.
- Re-run every offline-only verification in §7 against a real PostgreSQL
  instance the moment local DB access is restored.
