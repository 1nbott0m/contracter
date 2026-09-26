-- Virtual CONTRACTER stock is not an external purchase.  It is the bounded
-- inventory projection used by market availability and quote reservations.
-- New catalog rows must receive their policy target exactly once; existing
-- rows retain their current balance and reservations.
CREATE OR REPLACE FUNCTION ensure_virtual_warehouse_stock()
RETURNS integer
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    inserted_rows integer;
BEGIN
    INSERT INTO public.warehouse_stock (sku_id, available_units)
    SELECT sku.id, band.target_units
      FROM public.skus AS sku
      JOIN public.catalog_items AS item ON item.id = sku.catalog_item_id
      JOIN public.stock_policy_bands AS band
        ON band.rarity_code = item.rarity_code
       AND band.stock_policy_version_id = (
           SELECT policy.id
             FROM public.stock_policy_versions AS policy
            WHERE policy.activated_at IS NOT NULL
              AND policy.retired_at IS NULL
            ORDER BY policy.activated_at DESC, policy.id DESC
            LIMIT 1
       )
     WHERE sku.enabled
       AND item.enabled
    ON CONFLICT (sku_id) DO NOTHING;

    GET DIAGNOSTICS inserted_rows = ROW_COUNT;
    RETURN inserted_rows;
END;
$function$;

REVOKE ALL ON FUNCTION ensure_virtual_warehouse_stock() FROM PUBLIC;
DO $grant$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION ensure_virtual_warehouse_stock() TO contracter_runtime;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime') THEN
        GRANT EXECUTE ON FUNCTION ensure_virtual_warehouse_stock() TO contracter_admin_runtime;
    END IF;
END
$grant$;
