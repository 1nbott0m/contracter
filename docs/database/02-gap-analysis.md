# Gap Analysis — Database Layer

Severity key: **P0** money/item loss or corruption risk · **P1** critical
before production · **P2** important quality/architecture gap · **P3**
optimization/cleanup.

**No P0 found.** The core money/inventory paths (`post_ledger_transaction`,
`post_credit_adjustment`, `finalize_contract`) are already defended by DB
constraints, not just application code: a deferred trigger rejects any
unbalanced ledger transaction at commit regardless of which function wrote
it; `inventory_positions` has a single `CHECK` that makes "owned by two
places at once" a constraint violation, not just an untested code path;
append-only journals are enforced by trigger against both UPDATE/DELETE *and*
direct INSERT from any role but the table owner. This is confirmed both by
direct migration review and by `db/tests/001_invariants.sql`, which already
exercises most of these as executable SQLSTATE-level assertions (two-person
approval, idempotent/rejected-duplicate credit adjustments, unbalanced-ledger
rejection, append-only rejection, exact-10-input rejection).

## P1 — must fix before merging the affected open branches

1. **Migration version `0009` collides between two unmerged branches.**
   `feature/stock-risk-read-access` (`0009_stock_policy_read_grants.sql`) and
   `feature/collection-scarcity-engine` (`0009_collection_scarcity.sql`) both
   branched from the same `main` tip and independently claimed `0009`.
   Whichever merges first keeps the number; the other needs a rename to
   `0010_*.sql` (filename only — SQLx derives the version from the filename
   prefix, so this is a rename, not a data migration) before it can merge.
   *Evidence:* git-history sub-agent, confirmed by `git ls-tree` on both
   branch tips.

2. **`crates/economy-core/src/tradeup.rs::apply_collection_scarcity` trusts
   its input denominator without checking it**, unlike `select_outcome`
   (lines 251-253 of the same file), which does defensively verify every
   input outcome shares one denominator before trusting them.
   `apply_collection_scarcity` only reads `weight_numerator` from each
   `WeightedOutcome`. Today this is latent — the only producer of
   `WeightedOutcome` is `build_outcomes`, which does maintain a shared
   denominator, and every existing test feeds it straight through — but
   there is no guard against a future caller combining `WeightedOutcome`s
   from more than one `build_outcomes` call. Silent effect if it ever
   happens: wrong player-facing odds with no error and no panic. *Fix:*
   assert a shared `weight_denominator` at the top of the function and
   return `TradeupError::InvalidWeights` on mismatch, mirroring
   `select_outcome`'s own check. *Evidence:* code-quality sub-agent, verified
   by direct read of `tradeup.rs:162-210` and `251-253`.

3. **`crates/db/src/scarcity.rs::publish_collection_scarcity_snapshot` is not
   atomic on its own** — it performs three separate statements
   (`INSERT` snapshot header, `INSERT…SELECT` items, `SELECT
   publish_collection_scarcity_snapshot(...)`) on a bare `&mut PgConnection`
   with no internal transaction, unlike `ledger.rs::post_credit_adjustment`,
   which is exactly one statement and is therefore atomic regardless of
   caller discipline. Every current test wraps the call in an explicit
   rolled-back transaction, so atomicity holds only by caller convention
   today. If a future caller runs it on an autocommit connection and the
   process dies between statements, an orphaned unpublished snapshot row
   (and possibly its items) is left permanently in the append-only history —
   `current_collection_scarcity` (the live-weighting projection) cannot be
   corrupted this way since it only changes in the final atomic stored-proc
   call, which is why this is P1 rather than P0. *Fix:* change the
   signature to take `&mut Transaction<'_, Postgres>`, or begin/commit an
   internal transaction. *Evidence:* code-quality sub-agent, verified by
   direct read of `scarcity.rs:77-135`.

4. **`0009_collection_scarcity.sql` omits two patterns every sibling
   snapshot table already has.** Found by direct comparison against
   `valuation_snapshots`/`valuation_snapshot_items` (0004) and
   `publish_valuation_snapshot`'s grant (0005):
   - `collection_scarcity_snapshots` / `collection_scarcity_snapshot_items`
     are not added to 0005's append-only-guard `FOREACH` array, unlike
     `valuation_snapshot_items` — so, once merged, they will accept direct
     `UPDATE`/`DELETE` from the table owner (the migration/superuser role)
     and, more importantly, direct `INSERT` from *any* role, since the
     `require_journal_owner_insert` protection was never attached to them
     either.
   - No role is granted `EXECUTE` on `publish_collection_scarcity_snapshot`
     — `publish_valuation_snapshot` is explicitly granted to
     `contracter_admin_runtime` in 0005; the scarcity equivalent has no
     analogous grant anywhere, so as merged today **no application role
     could call it in production**.
   *Fix:* a follow-up migration (on the same branch, before merge) adding
   both tables to the append-only-guard arrays and granting `EXECUTE` on
   `publish_collection_scarcity_snapshot` to `contracter_admin_runtime`.

5. **No test anywhere in the repository exercises true concurrency** — two
   real, simultaneous PostgreSQL connections racing on the same resource.
   Every existing test (`db/tests/*.sql`, all `crates/db/tests/*.rs`) runs
   sequentially inside one connection/one transaction. This matters
   specifically because the project's own stated top risks — "100 users
   racing the last item," double-spend, double-issue, lost updates — are
   exactly the class of bug that sequential tests structurally cannot catch:
   `SELECT ... FOR UPDATE` lock-ordering can be correct in every sequential
   test and still deadlock or race under real concurrency. This is the
   single largest gap between "well-tested" (true, for sequential
   correctness) and "production-ready for concurrent load" (unproven).
   *Evidence:* direct read of every test file's structure; confirmed no
   `tokio::spawn`/multi-pool/multi-connection pattern exists anywhere in
   `crates/db/tests`.

## P2 — important, not blocking

6. **Redundant re-locking in `post_credit_adjustment`.** It locks
   `ledger_accounts {system, target}` `ORDER BY id FOR UPDATE` itself
   (0004, lines 77-81) and then calls `post_ledger_transaction`, which
   independently re-locks the same two accounts in the same order (0003).
   Not a deadlock risk (same table, same ascending order both times) and not
   a correctness bug, but it's two round trips to acquire the same locks and
   makes the actual lock-acquisition point non-obvious to a reader. Worth a
   comment or refactor when this code is next touched, not urgent on its
   own.

7. **Unchecked multiplication in `build_outcomes`.**
   `tradeup.rs:126` — `INPUT_COUNT as u64 * collection_outputs.len() as u64`
   is the one raw (non-`checked_`) multiplication amid an otherwise
   strictly checked-arithmetic function. Not realistically triggerable
   (`collection_outputs.len()` would need to approach `u64::MAX/10`, far
   beyond any possible allocation), but inconsistent with the rest of the
   file's defensive style.

8. **No genuine multi-connection concurrency test exists even for the
   already-shipped `finalize_contract`/`post_credit_adjustment` paths on
   `main`** — same root cause as finding 5, called out separately here
   because those two functions are already live (merged), not proposed.

## P3 — cleanup / documentation

9. Migration 0001–0005 lost their explicit `\set ON_ERROR_STOP on` /
   `BEGIN`/`COMMIT` wrapper between the commit that introduced them and the
   very next commit (both pre-publication, per git history) — current state
   is correct (SQLx wraps each migration in its own transaction already;
   `db/verify.sh`'s `psql --single-transaction` does the equivalent for the
   psql path), just noting it so nobody re-adds the redundant wrapper later
   under the impression it was accidentally dropped.

## What this audit deliberately did *not* attempt (and why)

- **`EXPLAIN ANALYZE` / index audit on realistic data volumes** — no
  `TEST_DATABASE_URL` is available in this environment (confirmed all
  session: every `#[ignore]` PostgreSQL integration test across every
  branch has never been run here, only compiled). This needs a real
  Postgres instance with seeded volume before it can be done honestly rather
  than guessed at.
- **Actual concurrent-connection chaos/deadlock reproduction** — same
  blocker; finding 5/8 above documents the *absence* of this test
  infrastructure, which is itself the actionable P1, but writing and
  *running* such a test needs the same real database connection.
- **Backup/recovery and observability** — CLAUDE.md and the repo scope this
  project as `db` + `economy-core` crates only, pre-deployment; no
  operational infrastructure (backup schedules, log shipping) exists in the
  repo to audit, and inventing one would be product/ops scope creep beyond
  "database layer."
