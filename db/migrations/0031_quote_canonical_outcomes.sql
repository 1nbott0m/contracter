-- One owner-bound, snapshot-consistent projection for quote outcome building.
-- It deliberately returns only public ids plus the minimum server-side facts
-- needed by economy-core; callers cannot use it to enumerate warehouse stock.
CREATE OR REPLACE FUNCTION public.read_quote_canonical_outcomes(
    p_user_id bigint,
    p_allocation_public_id uuid,
    p_input_inventory_item_ids bigint[]
)
RETURNS TABLE (
    input_count integer,
    input_collection_id bigint,
    input_rarity_code text,
    output_sku_id bigint,
    output_sku_public_id uuid,
    output_collection_id bigint,
    output_rarity_code text,
    output_weight_numerator bigint,
    output_weight_denominator bigint,
    candidate_inventory_item_id bigint,
    candidate_inventory_item_public_id uuid,
    candidate_min_float numeric,
    candidate_max_float numeric,
    candidate_canonical_float numeric,
    valuation_snapshot_item_id bigint,
    verified_price_microcredits bigint,
    warehouse_available_units integer,
    warehouse_reserved_units integer,
    stock_eligible boolean,
    risk_eligible boolean,
    valuation_snapshot_id bigint,
    stock_policy_version_id bigint,
    risk_policy_version_id bigint,
    formula_version text
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = public, pg_temp
AS $function$
WITH inputs AS (
    SELECT i.id, c.collection_id, c.rarity_code, r.rank
    FROM unnest(p_input_inventory_item_ids) AS requested(id)
    JOIN inventory_items i ON i.id = requested.id AND i.retired_at IS NULL
    JOIN inventory_positions ip ON ip.inventory_item_id = i.id
        AND ip.owner_user_id = p_user_id AND NOT ip.in_warehouse
    JOIN skus s ON s.id = i.sku_id AND s.enabled
    JOIN catalog_items c ON c.id = s.catalog_item_id AND c.enabled
    JOIN rarities r ON r.code = c.rarity_code
), input_shape AS (
    SELECT count(*)::integer AS input_count, min(collection_id) AS collection_id,
           min(rarity_code) AS rarity_code, min(rank) AS rank
    FROM inputs
    HAVING count(*) = cardinality(p_input_inventory_item_ids)
       AND count(*) BETWEEN 4 AND 10
       AND count(DISTINCT collection_id) = 1
       AND count(DISTINCT rarity_code) = 1
), active AS (
    SELECT rs.valuation_snapshot_id, rs.risk_policy_version_id,
           sp.id AS stock_policy_version_id, 'tradeup/v1'::text AS formula_version
    FROM risk_state rs
    JOIN stock_policy_versions sp ON sp.activated_at IS NOT NULL AND sp.retired_at IS NULL
    WHERE rs.singleton
    ORDER BY sp.activated_at DESC
    LIMIT 1
), outputs AS (
    SELECT ish.input_count, ish.collection_id, ish.rarity_code AS input_rarity_code,
           s.id AS output_sku_id, s.public_id AS output_sku_public_id,
           c.collection_id AS output_collection_id, c.rarity_code AS output_rarity_code,
           1::bigint AS output_weight_numerator, 1::bigint AS output_weight_denominator,
           wi.id AS candidate_inventory_item_id, wi.public_id AS candidate_inventory_item_public_id,
           c.min_float AS candidate_min_float, c.max_float AS candidate_max_float,
           wi.canonical_float, v.snapshot_item_id, v.verified_price_microcredits,
           ws.available_units, ws.reserved_units,
           (ws.available_units > ws.reserved_units AND ph.sku_id IS NULL) AS stock_eligible,
           (COALESCE(se.liability_microcredits,0) <= rp.maximum_item_liability_ratio * rs.liquid_reserve_microcredits
            AND COALESCE(ce.liability_microcredits,0) <= rp.maximum_collection_liability_ratio * rs.liquid_reserve_microcredits) AS risk_eligible,
           v.snapshot_id, a.stock_policy_version_id, a.risk_policy_version_id, a.formula_version
    FROM input_shape ish
    JOIN active a ON true
    JOIN risk_state rs ON rs.singleton
    JOIN risk_policy_versions rp ON rp.id = a.risk_policy_version_id
    JOIN catalog_items c ON c.collection_id = ish.collection_id
    JOIN rarities outr ON outr.code = c.rarity_code AND outr.rank = ish.rank + 1
        AND c.enabled
    JOIN skus s ON s.catalog_item_id = c.id AND s.enabled
    JOIN inventory_items wi ON wi.sku_id = s.id AND wi.retired_at IS NULL
    JOIN inventory_positions wp ON wp.inventory_item_id = wi.id AND wp.in_warehouse
    JOIN warehouse_stock ws ON ws.sku_id = s.id
    JOIN current_valuations v ON v.sku_id = s.id AND v.snapshot_id = a.valuation_snapshot_id
    LEFT JOIN price_halts ph ON ph.sku_id = s.id AND ph.lifted_at IS NULL
    LEFT JOIN risk_sku_exposures se ON se.sku_id = s.id
    LEFT JOIN risk_collection_exposures ce ON ce.collection_id = c.collection_id
)
SELECT * FROM outputs ORDER BY output_sku_id, candidate_inventory_item_id;
$function$;

REVOKE ALL ON FUNCTION public.read_quote_canonical_outcomes(bigint, uuid, bigint[]) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION public.read_quote_canonical_outcomes(bigint, uuid, bigint[])
            TO contracter_runtime;
    END IF;
END
$block$;
