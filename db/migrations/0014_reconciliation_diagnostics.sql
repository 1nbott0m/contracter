-- Read-only diagnostic functions that detect drift between a cached
-- projection and its append-only source of truth. Neither function
-- changes any data; both are SECURITY DEFINER purely so
-- contracter_admin_runtime can use them without a broad SELECT grant on
-- ledger_postings/collection_scarcity_snapshot_items (which it does not
-- otherwise have, matching this schema's existing least-privilege grants).
--
-- An empty result set means "no drift found," not "guaranteed consistent
-- forever" -- these are point-in-time diagnostics for operational use, not
-- constraints. Consistency is (and remains) primarily enforced by the
-- transactional writers themselves (post_ledger_transaction locking
-- ledger_accounts before writing both ledger_postings and ledger_balances
-- in the same statement; publish_collection_scarcity_snapshot doing the
-- same for its own pair of tables).

-- Every ledger_balances row must equal the sum of its account's
-- ledger_postings. A non-empty result means the cached balance and its
-- append-only source have diverged for that account.
CREATE OR REPLACE FUNCTION reconcile_ledger_balances()
RETURNS TABLE (
    account_id bigint,
    cached_balance_microcredits bigint,
    posted_balance_microcredits bigint
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT balance.account_id,
           balance.balance_microcredits,
           COALESCE(posting_totals.total, 0)
      FROM public.ledger_balances AS balance
      LEFT JOIN (
          SELECT posting.account_id, SUM(posting.amount_microcredits) AS total
            FROM public.ledger_postings AS posting
           GROUP BY posting.account_id
      ) AS posting_totals
        ON posting_totals.account_id = balance.account_id
     WHERE balance.balance_microcredits <> COALESCE(posting_totals.total, 0);
$function$;

-- Every current_collection_scarcity row must point at the most recently
-- published collection_scarcity_snapshot_items row for that collection
-- (highest snapshot_id). A non-empty result means "current" has fallen
-- behind the append-only snapshot history for that collection.
CREATE OR REPLACE FUNCTION reconcile_current_collection_scarcity()
RETURNS TABLE (
    collection_id bigint,
    current_snapshot_id bigint,
    latest_published_snapshot_id bigint
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT current_row.collection_id,
           current_row.snapshot_id,
           latest.snapshot_id
      FROM public.current_collection_scarcity AS current_row
      JOIN LATERAL (
          SELECT item.snapshot_id
            FROM public.collection_scarcity_snapshot_items AS item
           WHERE item.collection_id = current_row.collection_id
           ORDER BY item.snapshot_id DESC
           LIMIT 1
      ) AS latest ON true
     WHERE current_row.snapshot_id <> latest.snapshot_id;
$function$;

REVOKE ALL ON FUNCTION reconcile_ledger_balances() FROM PUBLIC;
REVOKE ALL ON FUNCTION reconcile_current_collection_scarcity() FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    ) THEN
        GRANT EXECUTE ON FUNCTION reconcile_ledger_balances()
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION reconcile_current_collection_scarcity()
            TO contracter_admin_runtime;
    END IF;
END;
$block$;
