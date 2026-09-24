# MAX DROP AUDIT — 15 000 CC

## Result

- STATUS: `IMPLEMENTED`
- VALUE: `15,000 CC`
- STORAGE UNIT: `OTHER` — `1 CC = 1,000,000 micro-CC units`
- DATABASE LIMIT: `15,000,000,000 microcredits`
- MINIMUM INPUT VALUE: `20 CC = 20,000,000 micro-CC units`
- INPUT COUNT: `4..=10`
- SERVER-SIDE: yes
- DATABASE-SIDE: yes
- BYPASS FOUND AFTER FIX: no bypass in the exercised application/DB paths

## Enforced at

1. `db/migrations/0033_quote_value_limits.sql`
   - `assert_quote_value_limits` rejects an input below `20,000,000`.
   - The same function rejects a negative outcome or one above
     `15,000,000,000`.
   - Constraint triggers protect `quote_inputs` and `quote_outcomes` on insert
     and update, so direct use of the atomic writer cannot bypass the rules.
2. `crates/application/src/quote.rs`
   - Input values and maximum exposure are derived from server-side canonical
     projections using checked arithmetic.
3. `db/migrations/0025_atomic_quote_lifecycle.sql`
   - Candidate ownership/availability, selected position, maximum exposure,
     reserve coverage and inventory reservations are revalidated atomically.
4. `db/migrations/0022_contract_input_count_range.sql` and
   `crates/application/src/quote.rs`
   - Contracts accept only four through ten unique input items.

## Bypass review

The checked route is:

```text
REQUEST
-> owner-bound allocation and input ids
-> canonical server projections
-> exact candidate weights and deterministic random selection
-> per-outcome absolute cap
-> complete signed quote document
-> atomic database insert and constraint triggers
-> owner-bound idempotent acceptance
-> ledger and inventory transition
```

Client-provided price, probability, result, user id and balance are not trusted.
Updating a stored outcome is also covered by the constraint trigger. The audit
did not find an exercised write path that can persist an outcome above the cap
under the application/runtime roles. A database owner can always alter schema
or disable triggers; owner credentials must not be available to the service.

## Regression tests

- `db/tests/006_quote_value_limits.sql`
  - rejects `19,999,999` microcredits;
  - accepts the exact `20,000,000` boundary;
  - accepts the exact `15,000,000,000` boundary;
  - rejects `15,000,000,001` and negative outcomes;
  - proves CC-to-micro-CC boundary arithmetic.
- `crates/application/src/quote.rs::input_count_must_be_four_through_ten`
- `crates/economy-core/tests/tradeup.rs`
  - minimum of four;
  - rejection outside `4..=10`;
  - exact normalization and float averaging.
- Full fresh-database `scripts/verify.sh`, which executes SQL invariants and the
  ignored quote/acceptance integration suite.

## Conclusion

The 20 CC floor and 15,000 CC absolute result cap are server/database
invariants, not UI conventions. They are enforced at both boundaries and have
boundary regressions. RUB is only a future funding input and cannot directly
set an internal balance. This conclusion applies to the audited repository paths;
production role grants must continue to exclude table-owner/schema privileges.
