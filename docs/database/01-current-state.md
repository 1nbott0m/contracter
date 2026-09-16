# Current State — Database Layer

Audited against `origin/main` (tip `2235f2d`) plus four open, unmerged branches:
`feature/stock-risk-read-access` (tip `28d9288`), `feature/contract-read-access`
(tip `3506583`), `feature/quote-read-access` (tip `f8a3786`),
`feature/collection-scarcity-engine` (tip `f786f56`). Sourced from direct file
reads plus two independent sub-agent passes (full `git log --all` mining;
adversarial Rust code-quality review across all five refs).

## 1. What's merged into `main` today

**Migrations 0001–0008** (`db/migrations/`), applied in order by `sqlx::migrate!`:

| # | File | Owns |
|---|------|------|
| 0001 | `extensions_and_lookups.sql` | `pgcrypto`; lookup tables (`rarities`, `ledger_account_kinds`, `critical_action_types`, `inventory_event_kinds`, `seed_event_kinds`, `quote_statuses`, `contract_statuses`, `sale_validity_reasons`, `price_halt_reasons`, `currencies`, `grant_kinds`) — all regex-checked, rows-not-enums per CLAUDE.md |
| 0002 | `identity_and_admin.sql` | `users`, invitations/sessions/recovery, `administrators`, two-person `critical_actions` + approval/execution event tables, immutability trigger on `critical_actions`, approval-validation trigger (active approver, not self-approval, hash match, 24h window, recovery freeze) |
| 0003 | `catalog_inventory_ledger.sql` | `collections`/`wear_bands`/`catalog_items`/`skus`, `inventory_items`/`inventory_positions` (single-owner-XOR-warehouse CHECK), `inventory_transfer_events`, `ledger_accounts`/`ledger_transactions`/`ledger_postings`/`ledger_balances`, deferred balanced-transaction trigger, `post_ledger_transaction()` / `post_credit_adjustment()`, `credit_adjustment_events` |
| 0004 | `prices_stock_quotes_contracts.sql` | pricing evidence/snapshots/halts, `stock_policy_versions`/`stock_policy_bands`, `warehouse_stock`, `risk_policy_versions`/`risk_state` (singleton, seeded), seed-commitment chain, `tradeup_quotes`/`quote_inputs`/`quote_outcomes`/`quote_candidate_reservations`/`quote_risk_exposures`, `contracts`/`contract_inputs`/`contract_outcomes`, allocation-claim + quote-promotion triggers, `publish_valuation_snapshot()`, `finalize_contract()` |
| 0005 | `append_only_guards.sql` | `reject_append_only_mutation()` (blocks UPDATE/DELETE on 16 journal tables) + `require_journal_owner_insert()` (blocks direct INSERT unless `current_user` is the table owner — i.e. only a `SECURITY DEFINER` function running as owner may write), `approve_critical_action()`, explicit `REVOKE INSERT/UPDATE/DELETE/TRUNCATE ... FROM PUBLIC` on all 16 journal tables, three-role grant scaffold (`contracter_runtime`, `contracter_admin_runtime`, `contracter_readonly`) |
| 0006 | `credit_adjustment_idempotency.sql` | `post_credit_adjustment` execution-key idempotency + advisory xact lock |
| 0007 | `inventory_availability_views.sql` | `available_user_inventory`/`available_warehouse_inventory` security-barrier views (excludes locked/reserved/retired), runtime grants |
| 0008 | `pricing_read_grants.sql` | runtime `SELECT` on price/valuation tables |

**Rust `db` crate on `main`:** `catalog.rs`, `inventory.rs`, `ledger.rs`, `pricing.rs`
(all read-only except `ledger.rs::post_credit_adjustment`, which wraps the
0006 stored procedure), `config.rs`, `database.rs` (`Database`, `DatabaseError`
with SQLSTATE-preserving `database_code()`, `MIGRATOR`), `ids.rs` (transparent
`i64`/`Uuid` newtypes per entity).

**Rust `economy-core` crate:** `tradeup.rs` (pure trade-up math: input
validation, weighted-outcome construction, deterministic commit-reveal
selection via rejection sampling), `pricing.rs` (trimmed-mean valuation,
buyback/spread math, LCM-based exact-fraction arithmetic — hardened by two
2026-09-13 fix commits, see §3), `policy.rs` (stock/risk policy value
objects). No `f32`/`f64` usage anywhere in either crate; money is
`bigint` microcredits end-to-end; wear floats are `numeric(9,8)` /
`rust_decimal::Decimal` end-to-end.

**Roles:** three PostgreSQL roles are referenced by every `GRANT`/`REVOKE`
block, each guarded by `IF EXISTS (SELECT 1 FROM pg_roles ...)` since managed
Postgres often denies `CREATEROLE` to the migration user — roles are
provisioned externally, not by a migration:
- `contracter_runtime` — narrow `SELECT` on read-path tables/views + `EXECUTE` on `finalize_contract` only. No journal table is directly writable even by this role (0005's append-only guard blocks it structurally, not just by grant).
- `contracter_admin_runtime` — `SELECT` on admin/ledger/audit tables + `EXECUTE` on `post_credit_adjustment`, `approve_critical_action`, `publish_valuation_snapshot`.
- `contracter_readonly` — `SELECT` on every table, for reporting/analytics.

## 2. Open, unmerged branches (proposed, not yet part of the source of truth)

| Branch | Adds | Migration | Depends on |
|---|---|---|---|
| `feature/quote-read-access` | `crates/db/src/quotes.rs` — read `tradeup_quotes`/`quote_inputs`/`quote_outcomes` | none | main |
| `feature/stock-risk-read-access` | `crates/db/src/stock.rs` — read `warehouse_stock`/active `stock_policy_*`/`risk_state` | `0009_stock_policy_read_grants.sql` | main |
| `feature/contract-read-access` | `crates/db/src/contracts.rs` — read `contracts`/`contract_inputs`/`contract_outcomes` | none | `feature/quote-read-access` |
| `feature/collection-scarcity-engine` | `crates/db/src/scarcity.rs` + `economy-core::tradeup::apply_collection_scarcity` | `0009_collection_scarcity.sql` | main |

All four are strictly additive (new files, new grants, one new migration
each) — none edit an already-applied migration or existing business logic.
Every read function returns `Option`/`Vec` with `Ok(None)`/empty for unknown
IDs, none perform writes except the two explicitly listed above.

## 3. Notable history (from git-mining sub-agent, verified)

- **Two real arithmetic-correctness fixes**, both 2026-09-13, both in
  `economy-core::pricing`: `16ea67e` replaced an equal-denominator assumption
  with LCM-based exact-fraction math and added `checked_add` for
  `input_rarity + 1`; `0ffffbf` GCD-reduces before an `i128 → Decimal`
  conversion to avoid a precision/panic edge case (regression test
  `large_equivalent_probability_does_not_panic_during_spread_calculation`
  added). Both are genuine bug fixes with regression tests, not workarounds.
- **Test-fixture rarity-code mismatch fix** (`7b0d1a2`): a test used
  `"mil_spec"` while real seed data and the DB regex use `"mil-spec"`.
- **Test-fixture timestamp race fix** (`28d9288`): a stock-policy test's
  "active" fixture needed a future `activated_at` to reliably outrank a
  pre-existing seeded active row when ordered `DESC`.
- **Zero TODO/FIXME/HACK/XXX/`unimplemented!`/`todo!`** anywhere in `.rs` or
  `.sql`, on any of the 7 refs checked.
- **No secrets ever committed** — `.env` never appears in history on any
  branch.
- **`CLAUDE.md` has never changed** since its single introducing commit
  (`8694873`) and is byte-identical across every branch — it is a stable,
  uncontested source of truth for this audit.
- **No revert commits** exist anywhere in the history.

## 4. What is already correctly enforced at the database level (not just in Rust)

- Exactly-10-unique-inputs for a contract: checked in `economy-core::tradeup::validate_inputs` *and* independently re-checked inside `finalize_contract` (0004) before any write — genuine defense in depth, not a gap.
- Append-only journals: structurally enforced by trigger (blocks UPDATE/DELETE unconditionally, blocks INSERT from any role except the table owner), not merely by convention or by withholding an app-level `UPDATE` code path.
- Ledger balance: every `ledger_transactions` row is forced balanced-to-zero by a deferred constraint trigger checked at commit, not just by the writer function's own arithmetic.
- Idempotency: `post_ledger_transaction` and `post_credit_adjustment` both take an idempotency/execution key, advisory-lock on it, and return the prior result unchanged on a repeat with an identical payload (and reject a repeat with a *different* payload under the same key, SQLSTATE `23505`).
- Single-owner-or-warehouse for every item instance: a single `CHECK` on `inventory_positions` (`(owner_user_id IS NOT NULL)::int + in_warehouse::int = 1`) makes the "one instance, one unambiguous state" rule from the audit brief structurally impossible to violate, not just application-enforced.
- Two-person critical-action approval: proposer≠approver, active-approver-only, and payload-hash pinning are enforced by a `BEFORE INSERT` trigger on the approval table itself, not by the calling function's discretion.
