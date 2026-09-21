# Database Invariants

Format: `INVARIANT → DB CONSTRAINT → APPLICATION CHECK → TRANSACTION/LOCK → TEST`.
Only critical (money/inventory/security) invariants are listed. Source
references point to the current, merged state on `main` unless marked
`[open PR]`.

| # | Invariant | DB constraint | App check | Transaction / lock | Test |
|---|---|---|---|---|---|
| 1 | One item instance has exactly one location: a user or the warehouse, never both, never neither | `inventory_positions` CHECK `(owner_user_id IS NOT NULL)::int + in_warehouse::int = 1` (0003) | none needed — DB-level | row lock via `finalize_contract`'s `ORDER BY inventory_item_id FOR UPDATE` (0004) | not yet exercised by a concurrency test (gap-analysis #5) |
| 2 | A ledger transaction always balances to zero | deferred `CONSTRAINT TRIGGER ledger_postings_balanced` (0003), checked at commit regardless of which function wrote the postings | `economy-core::pricing` computes exact balanced entries before calling `post_ledger_transaction` | `post_ledger_transaction` locks all referenced `ledger_accounts` `ORDER BY account.id FOR UPDATE` (0003) | `db/tests/001_invariants.sql`: "an unbalanced ledger transaction is rejected" (SQLSTATE 23514) |
| 3 | A contract consumes 4–10 unique inputs of one rarity | none directly on `contract_inputs` row count (checked procedurally) | `economy_core::tradeup::validate_inputs` (`MIN_INPUT_COUNT == 4`, `MAX_INPUT_COUNT == 10`, single rarity) | `finalize_contract` re-validates cardinality/uniqueness *before* any lookup or lock (0004, upgraded in 0022) | `economy-core/tests/tradeup.rs::rejects_input_counts_outside_the_four_to_ten_range`, `rejects_mixed_input_rarities`; `db/tests/001_invariants.sql`: 3-, 4-, and 11-input checks |
| 4 | An idempotency/execution key never creates two business operations | `UNIQUE` on `ledger_transactions.idempotency_key`, `credit_adjustment_events.execution_key`, `quote_acceptance_events.quote_id`/`idempotency_key` | caller supplies a client-chosen key | `pg_advisory_xact_lock(hashtextextended(key, ...))` taken *before* the existence check, in every idempotent writer (0003, 0006) | `db/tests/001_invariants.sql`: reuse-with-different-payload rejection (23505); `crates/db/tests/ledger.rs::credit_adjustment_updates_balance_and_is_idempotent` |
| 5 | A journal/history table is append-only — no UPDATE, no DELETE, ever | `reject_append_only_mutation()` trigger on 16 tables (0005) | none — DB-level, cannot be bypassed by app code | statement-level `BEFORE UPDATE OR DELETE` trigger | `db/tests/001_invariants.sql::assert_append_only` × 10 tables |
| 6 | A journal/history table cannot be written by direct `INSERT` from anything but its owning `SECURITY DEFINER` function | `require_journal_owner_insert()` trigger comparing `current_user` to the table owner (0005) | n/a — DB-level | statement-level `BEFORE INSERT` trigger | not directly asserted by an executable test today (only indirectly, since every writer test goes through the function); **gap:** no test attempts a direct `INSERT` as `contracter_runtime` and asserts rejection |
| 7 | A critical (two-person) action requires an approver who is active, is not the proposer, and approves the exact original payload within 24h | `validate_critical_action_approval()` `BEFORE INSERT` trigger (0002) + immutability trigger on `critical_actions` itself | `post_credit_adjustment` independently re-validates the linked `critical_actions` row before use (0004/0006) | trigger takes `FOR UPDATE` on the action row, `FOR SHARE` on the approver | `db/tests/001_invariants.sql`: self-approval rejection, expired-window rejection, valid-second-approval success |
| 8 | A quote/allocation is accepted at most once; a retry with the same idempotency key returns the prior result | `UNIQUE (quote_id)` and `UNIQUE (idempotency_key)` on `quote_acceptance_events`; `finalize_contract` checks-then-returns before any write (0004) | n/a | `pg_advisory_xact_lock` on the idempotency key first | `db/tests/001_invariants.sql::has_unique_single_column` on `quote_acceptance_events.quote_id`; no direct `finalize_contract` retry test exists yet since it needs full quote/stock fixtures (**gap**, not yet built by any read-access PR) |
| 9 | Money never uses binary float; scale and rounding are explicit and exact | every money column is `bigint` microcredits; `numeric(9,8)` for floats/ratios | `rust_decimal::Decimal` end-to-end in `economy-core`; zero `f32`/`f64` in `crates/db`/`crates/economy-core` (verified by code-quality sub-agent, all branches) | n/a | `economy-core/tests/pricing.rs` (rounding, spread, trimmed mean); `crates/db/tests/*.rs` Decimal-precision tests in `catalog.rs`/`contracts.rs`/`quotes.rs` |
| 10 | A collection's scarcity-damped weight is never negative and never exceeds its undamped baseline `[open PR]` | `CHECK (0 <= weight_multiplier_numerator <= weight_multiplier_denominator)` on both scarcity tables (0009, open) | `apply_collection_scarcity` rejects an out-of-range multiplier defensively (`TradeupError::InvalidWeights`) | n/a (pure function, no lock) | `economy-core/tests/tradeup.rs::draining_stock_cannot_increase_a_collections_own_weight` (monotonicity property) — **but see gap-analysis #2**: the function does not yet validate its *own* input denominator |
| 11 | Global stock (`warehouse_stock.available_units`) never goes negative | `CHECK (available_units >= 0)`, `CHECK (reserved_units <= available_units)` (0004) | `finalize_contract` locks `warehouse_stock` `ORDER BY sku_id FOR UPDATE` and re-checks `available_units > 0 AND reserved_units > 0` before decrementing, erroring `23514` otherwise | same lock as above | not yet exercised by a concurrency test (gap-analysis #5); sequential correctness only |

## Lock-ordering map (derived by tracing every `FOR UPDATE`/advisory-lock site)

Every multi-resource writer acquires locks in one of two safe patterns, and
no two writers acquire overlapping resource classes in conflicting relative
order — traced explicitly, not assumed:

- **Global serialization point:** `finalize_contract` and
  `publish_valuation_snapshot` both take `FOR UPDATE` on the singleton
  `risk_state` row *first*, before touching anything else. Two concurrent
  calls to either function cannot deadlock each other because the second
  simply blocks on that single row until the first commits or rolls back.
- **Sorted-set locking:** every function that locks more than one row of the
  same table (`ledger_accounts` in `post_credit_adjustment`/
  `post_ledger_transaction`; `inventory_positions` and `warehouse_stock` in
  `finalize_contract`) does so via `... ORDER BY <pk> FOR UPDATE`, the
  standard deadlock-avoidance pattern — two transactions locking overlapping
  rows in the same table always request them in the same relative order.
- **No cross-table cycle found:** `post_credit_adjustment` never touches
  `inventory_positions`/`warehouse_stock`; `finalize_contract`'s optional
  ledger step reuses the same `ORDER BY account.id` convention. No pair of
  reviewed writers locks two shared tables in opposite order.

This is a **static trace, not a proven-under-load result** — it explains why
no deadlock is expected from the code as written, not that one has been
observed absent under real concurrent load (see gap-analysis #5).
