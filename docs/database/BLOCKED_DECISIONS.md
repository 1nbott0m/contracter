# Blocked decisions

Findings from an independent adversarial review of the DB layer
(`crates/db`, `crates/economy-core`, `db/migrations`) that need a product or
architecture decision outside DATABASE ONLY scope before they can be
resolved in this repository. Each entry says what was found, why it isn't
fixed yet, and what would unblock it.

## 1. No per-request caller identity reaches the database

**Finding.** `finalize_contract` and `approve_critical_action`
(`db/migrations/0004_prices_stock_quotes_contracts.sql`,
`db/migrations/0005_append_only_guards.sql`) trust their bigint arguments
completely. `finalize_contract` never compares the calling session to the
quote's `user_id`; `approve_critical_action`'s dual-control check only
verifies the proposer and approver differ, never that the caller actually
*is* the approver it names. Because every player shares one
`contracter_runtime` credential and every admin shares one
`contracter_admin_runtime` credential, nothing in the database itself can
tell two different end users apart.

**Partial fix applied.** `db/migrations/0009_security_hardening.sql` adds
`finalize_contract_for_user` / `approve_critical_action_as_admin`: thin
`SECURITY DEFINER` wrappers that require an explicit
`p_calling_user_id` / `p_calling_admin_id` argument and reject a mismatch
before delegating to the underlying function, with `EXECUTE` moved from the
raw functions to these wrappers. This closes the "a handler simply forgot
to check ownership" class of bug and makes identity an explicit, auditable
argument at the one place every such call must pass through.

**What remains blocked.** The wrapper still trusts that the caller passes a
*genuinely authenticated* id — it cannot verify that on its own. Closing
that residual gap requires one of:

- Per-user (or per-admin) database roles, with the application
  authenticating to Postgres as that specific role — a significant
  connection-pooling and infrastructure change.
- Row-level security policies keyed off a session-local setting
  (`current_setting('app.user_id')`) that the *application* sets
  immediately after authenticating a request — workable, but the
  guarantee is only as strong as the application code that sets it, and
  deciding where/how that setting gets populated is an
  authentication-architecture decision, not a database one.
- Leaving it exactly as it is now (explicit argument, trusted caller) and
  documenting it as a hard requirement on whichever backend eventually
  calls these functions.

This is a product/architecture decision for whoever designs the
authentication layer, not something DATABASE ONLY scope can resolve
unilaterally. No backend exists yet in this repository to make the call
either way.

## 2. `valuation_snapshots` / `current_valuations` (and the scarcity
   equivalents) cannot get the standard append-only guard trigger

**Finding.** Every other journal-style table in this schema
(`ledger_transactions`, `quote_inputs`, `valuation_snapshot_items`,
`collection_scarcity_snapshot_items`, etc.) is protected by
`journal_no_update_delete` / `journal_owner_insert_only`
(`db/migrations/0005_append_only_guards.sql`), which unconditionally
rejects any `UPDATE`/`DELETE` and any `INSERT` not run as the table owner.
`valuation_snapshots`, `current_valuations`,
`collection_scarcity_snapshots`, and `current_collection_scarcity` have no
such trigger.

**Why the standard fix doesn't apply.** `publish_valuation_snapshot` and
`publish_collection_scarcity_snapshot` both legitimately `UPDATE` their
header row's `published_at` field (a one-time NULL -> timestamp
transition) and `UPSERT` the `current_*` pointer row via
`INSERT ... ON CONFLICT DO UPDATE`. Attaching the blunt
`journal_no_update_delete` trigger (which rejects *every* `UPDATE`
unconditionally, including from the table owner) would break both
functions outright. This is very likely why the original author left
these four tables out of the guarded list, not an oversight.

**Current exposure.** Low. Neither `contracter_runtime` nor
`contracter_admin_runtime` is granted `INSERT`/`UPDATE`/`DELETE` on any of
these four tables (verified across all migrations) — only the
`SECURITY DEFINER` publish functions, running as the table owner, can
write to them. The only way to exploit this gap today is a manual
superuser/table-owner connection making a mistake, or a future migration
that widens grants without noticing this precedent.

**What would unblock it.** A column-aware guard — e.g. a trigger that
allows `UPDATE` only when it is setting `published_at` from `NULL` to
non-`NULL` and touches no other column, and separately allows the
`current_*` pointer tables' `ON CONFLICT DO UPDATE` shape specifically —
would close this properly. That is a real (if narrow) design task, not a
one-line fix, and is deferred rather than rushed into this review pass.

## 3. `find_ledger_balance` takes a bare, sequential internal id with no
   ownership parameter

**Finding.** `crates/db/src/ledger.rs::find_ledger_balance` accepts a
`LedgerAccountId` (a plain `GENERATED ALWAYS AS IDENTITY bigint`) with no
caller/owner check. If a future API handler ever forwards a
client-supplied integer straight into this function without an extra
`account.owner_user_id == session.user_id` check of its own, sequential ids
make balance enumeration trivial (`1, 2, 3, ...`).

**Why it isn't fixed here.** This is a library API design question, not a
bug: `crates/db` is intentionally a thin, caller-trusting data-access
layer (every other read function in this crate has the same shape), and
there is no HTTP layer yet to demonstrate what the real call site would
need. Every other public identifier in this schema deliberately uses the
unguessable `PublicId(Uuid)` wrapper instead of a raw sequential id
specifically to avoid this; `LedgerAccountId` is the one exception.

**What would unblock it.** Either accept this as a documented convention
(caller/API layer is responsible for the ownership check, same as
`finalize_contract`'s residual trust boundary in blocked decision 1) or
add a second, owner-checked accessor
(`find_ledger_balance_for_user(user_id, account_id)`) once a real caller
exists to define what "owner-checked" should mean for system accounts vs.
user accounts. Deferred until an API layer exists to make that call
concrete.
