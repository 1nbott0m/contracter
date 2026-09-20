-- Migration 0019 protected the normal unpublished -> published transition,
-- but a direct INSERT may also provide published_at. Since valuation items
-- require an existing snapshot row, an already-published snapshot can never
-- be valid at insertion time.
CREATE OR REPLACE FUNCTION reject_empty_valuation_snapshot_publish()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
BEGIN
    IF NEW.published_at IS NULL THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'UPDATE' AND OLD.published_at IS NOT NULL THEN
        RETURN NEW;
    END IF;

    IF NOT EXISTS (
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
BEFORE INSERT OR UPDATE OF published_at ON valuation_snapshots
FOR EACH ROW
EXECUTE FUNCTION reject_empty_valuation_snapshot_publish();
