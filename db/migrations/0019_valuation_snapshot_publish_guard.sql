-- A price-less snapshot is not a valuation. Publishing one would invalidate
-- active quotes and advance risk_state without populating current_valuations.
-- Keep this at the table boundary, rather than only in
-- publish_valuation_snapshot(), so every current and future publication path
-- preserves the same invariant.
CREATE OR REPLACE FUNCTION reject_empty_valuation_snapshot_publish()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
BEGIN
    IF OLD.published_at IS NULL
       AND NEW.published_at IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM public.valuation_snapshot_items AS snapshot_item
           WHERE snapshot_item.snapshot_id = NEW.id
       ) THEN
        RAISE EXCEPTION 'valuation snapshot must contain at least one item before publication'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$function$;

REVOKE ALL ON FUNCTION reject_empty_valuation_snapshot_publish() FROM PUBLIC;

DROP TRIGGER IF EXISTS valuation_snapshot_requires_items
    ON valuation_snapshots;
CREATE TRIGGER valuation_snapshot_requires_items
BEFORE UPDATE OF published_at ON valuation_snapshots
FOR EACH ROW
WHEN (OLD.published_at IS NULL AND NEW.published_at IS NOT NULL)
EXECUTE FUNCTION reject_empty_valuation_snapshot_publish();
