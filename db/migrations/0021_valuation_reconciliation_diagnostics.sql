-- Read-only valuation diagnostics. The publication trigger is the primary
-- protection, while these functions surface historical or owner-level drift
-- without changing any rows.

-- A published valuation snapshot must contain at least one valuation item.
-- A non-empty result identifies data that predates, or bypassed, the
-- publication guard.
CREATE OR REPLACE FUNCTION reconcile_published_valuation_snapshots()
RETURNS TABLE (
    snapshot_id bigint
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT snapshot.id
      FROM public.valuation_snapshots AS snapshot
     WHERE snapshot.published_at IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
             FROM public.valuation_snapshot_items AS item
            WHERE item.snapshot_id = snapshot.id
       )
     ORDER BY snapshot.id;
$function$;

-- Every current valuation is a cached projection of exactly one item in a
-- published valuation snapshot. One row is returned per detected mismatch so
-- an operator can distinguish the broken field without broad table access.
CREATE OR REPLACE FUNCTION reconcile_current_valuations()
RETURNS TABLE (
    sku_id bigint,
    current_snapshot_id bigint,
    current_snapshot_item_id bigint,
    source_snapshot_id bigint,
    source_sku_id bigint,
    cached_price_microcredits bigint,
    source_price_microcredits bigint,
    issue_code text
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT current_value.sku_id,
           current_value.snapshot_id,
           current_value.snapshot_item_id,
           item.snapshot_id,
           item.sku_id,
           current_value.verified_price_microcredits,
           item.verified_price_microcredits,
           issue.code
      FROM public.current_valuations AS current_value
      JOIN public.valuation_snapshot_items AS item
        ON item.id = current_value.snapshot_item_id
      JOIN public.valuation_snapshots AS snapshot
        ON snapshot.id = item.snapshot_id
      CROSS JOIN LATERAL (
          VALUES
              ('snapshot_id_mismatch'::text, current_value.snapshot_id IS DISTINCT FROM item.snapshot_id),
              ('sku_id_mismatch'::text, current_value.sku_id IS DISTINCT FROM item.sku_id),
              ('price_mismatch'::text, current_value.verified_price_microcredits IS DISTINCT FROM item.verified_price_microcredits),
              ('source_snapshot_unpublished'::text, snapshot.published_at IS NULL)
      ) AS issue(code, is_drift)
     WHERE issue.is_drift
     ORDER BY current_value.sku_id, issue.code;
$function$;

REVOKE ALL ON FUNCTION reconcile_published_valuation_snapshots() FROM PUBLIC;
REVOKE ALL ON FUNCTION reconcile_current_valuations() FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    ) THEN
        GRANT EXECUTE ON FUNCTION reconcile_published_valuation_snapshots()
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION reconcile_current_valuations()
            TO contracter_admin_runtime;
    END IF;
END;
$block$;
