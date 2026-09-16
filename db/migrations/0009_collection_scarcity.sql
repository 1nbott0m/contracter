CREATE TABLE IF NOT EXISTS collection_scarcity_snapshots (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    formula_version text NOT NULL,
    snapshot_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    CHECK (formula_version <> ''),
    CHECK (published_at IS NULL OR published_at >= created_at)
);

CREATE TABLE IF NOT EXISTS collection_scarcity_snapshot_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    snapshot_id bigint NOT NULL REFERENCES collection_scarcity_snapshots(id),
    collection_id bigint NOT NULL REFERENCES collections(id),
    available_units_total integer NOT NULL,
    target_units_total integer NOT NULL,
    weight_multiplier_numerator bigint NOT NULL,
    weight_multiplier_denominator bigint NOT NULL,
    UNIQUE (snapshot_id, collection_id),
    CHECK (available_units_total >= 0),
    CHECK (target_units_total >= 0),
    CHECK (weight_multiplier_denominator > 0),
    CHECK (weight_multiplier_numerator >= 0),
    CHECK (weight_multiplier_numerator <= weight_multiplier_denominator)
);

CREATE TABLE IF NOT EXISTS current_collection_scarcity (
    collection_id bigint PRIMARY KEY REFERENCES collections(id),
    snapshot_id bigint NOT NULL REFERENCES collection_scarcity_snapshots(id),
    snapshot_item_id bigint NOT NULL UNIQUE REFERENCES collection_scarcity_snapshot_items(id),
    weight_multiplier_numerator bigint NOT NULL,
    weight_multiplier_denominator bigint NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (weight_multiplier_denominator > 0),
    CHECK (weight_multiplier_numerator >= 0),
    CHECK (weight_multiplier_numerator <= weight_multiplier_denominator)
);

-- A collection's real-stock damping factor: min(available, target) / target,
-- summed per collection over its enabled SKUs against the currently active
-- stock policy's per-rarity target. A collection with no target coverage
-- (target_units_total = 0) is left undamped (1/1) rather than zeroed, so
-- gaps in stock-policy coverage never silently disable a collection.
--
-- Computes, inserts, and publishes the snapshot in one call rather than
-- taking a pre-created snapshot id: every other writer in this schema
-- (post_credit_adjustment, finalize_contract, publish_valuation_snapshot)
-- does its INSERTs inside its own SECURITY DEFINER body so the caller only
-- ever needs EXECUTE, never direct INSERT on the underlying tables. An
-- earlier version of this function split "compute and insert items" into
-- application code and "flip published_at" into this function, which left
-- the item-insert step running as the caller's own role with no INSERT
-- grant on collection_scarcity_snapshot_items -- unusable by any role as
-- written. Returns the new snapshot's id.
CREATE OR REPLACE FUNCTION publish_collection_scarcity_snapshot(p_formula_version text)
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    -- Named v_snapshot_id, not snapshot_id: a bare `snapshot_id` in the SQL
    -- below is ambiguous against the identically-named table column and
    -- Postgres does not resolve that in the variable's favor. This is only
    -- caught when the statement actually executes, not at CREATE FUNCTION
    -- time, so it slipped through review once already.
    v_snapshot_id bigint;
BEGIN
    IF p_formula_version IS NULL OR p_formula_version = '' THEN
        RAISE EXCEPTION 'formula version is required' USING ERRCODE = '23514';
    END IF;

    INSERT INTO public.collection_scarcity_snapshots (formula_version, snapshot_at)
    VALUES (p_formula_version, clock_timestamp())
    RETURNING id INTO v_snapshot_id;

    INSERT INTO public.collection_scarcity_snapshot_items (
        snapshot_id,
        collection_id,
        available_units_total,
        target_units_total,
        weight_multiplier_numerator,
        weight_multiplier_denominator
    )
    WITH collection_sku_targets AS (
        SELECT catalog_items.collection_id, skus.id AS sku_id, band.target_units
          FROM public.skus
          JOIN public.catalog_items ON catalog_items.id = skus.catalog_item_id
          JOIN public.stock_policy_bands AS band
            ON band.rarity_code = catalog_items.rarity_code
           AND band.stock_policy_version_id = (
               SELECT id FROM public.stock_policy_versions
                WHERE activated_at IS NOT NULL AND retired_at IS NULL
                ORDER BY activated_at DESC, id DESC LIMIT 1)
         WHERE catalog_items.enabled AND skus.enabled
    ),
    aggregated AS (
        SELECT collection_sku_targets.collection_id,
               COALESCE(SUM(warehouse_stock.available_units), 0)::integer
                   AS available_units_total,
               SUM(collection_sku_targets.target_units)::integer AS target_units_total
          FROM collection_sku_targets
          LEFT JOIN public.warehouse_stock
            ON warehouse_stock.sku_id = collection_sku_targets.sku_id
         GROUP BY collection_sku_targets.collection_id
    )
    SELECT v_snapshot_id,
           aggregated.collection_id,
           aggregated.available_units_total,
           aggregated.target_units_total,
           LEAST(aggregated.available_units_total, aggregated.target_units_total),
           aggregated.target_units_total
      FROM aggregated
     WHERE aggregated.target_units_total > 0;

    UPDATE public.collection_scarcity_snapshots
       SET published_at = clock_timestamp()
     WHERE id = v_snapshot_id;

    INSERT INTO public.current_collection_scarcity (
        collection_id,
        snapshot_id,
        snapshot_item_id,
        weight_multiplier_numerator,
        weight_multiplier_denominator,
        updated_at
    )
    SELECT item.collection_id,
           item.snapshot_id,
           item.id,
           item.weight_multiplier_numerator,
           item.weight_multiplier_denominator,
           clock_timestamp()
      FROM public.collection_scarcity_snapshot_items AS item
     WHERE item.snapshot_id = v_snapshot_id
    ON CONFLICT (collection_id) DO UPDATE
    SET snapshot_id = EXCLUDED.snapshot_id,
        snapshot_item_id = EXCLUDED.snapshot_item_id,
        weight_multiplier_numerator = EXCLUDED.weight_multiplier_numerator,
        weight_multiplier_denominator = EXCLUDED.weight_multiplier_denominator,
        updated_at = EXCLUDED.updated_at;

    RETURN v_snapshot_id;
END;
$function$;

REVOKE ALL ON FUNCTION publish_collection_scarcity_snapshot(text) FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT SELECT ON collection_scarcity_snapshots,
                        collection_scarcity_snapshot_items,
                        current_collection_scarcity
            TO contracter_runtime;
    END IF;
END;
$block$;
