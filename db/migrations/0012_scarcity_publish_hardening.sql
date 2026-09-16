-- Adversarial-review fixes for publish_collection_scarcity_snapshot:
--
-- 1. Concurrent publishes were not serialized against each other. Two
--    overlapping calls (a retried admin request, or two scheduled
--    recomputations) each race their own `ON CONFLICT (collection_id) DO
--    UPDATE` into current_collection_scarcity; without a shared lock,
--    whichever transaction's upsert happens to commit last wins, which is a
--    function of wall-clock scheduling, not of which snapshot is actually
--    newer -- current_collection_scarcity could silently end up pointing at
--    an older snapshot than the append-only history's latest row. Every
--    other "flip current state" writer in this schema already takes a lock
--    before writing (publish_valuation_snapshot and finalize_contract take
--    `risk_state FOR UPDATE`; post_credit_adjustment takes a
--    pg_advisory_xact_lock on its idempotency key); this one had neither an
--    idempotency key nor a lock. Fixed with a fixed-key pg_advisory_xact_lock
--    so all publishes serialize and the last one to commit is always the
--    last one created.
--
-- 2. The active stock-policy-version lookup never checked that
--    `activated_at` had actually arrived (`activated_at <= clock_timestamp()`
--    was missing), so a version scheduled to activate in the future could
--    outrank a genuinely-currently-active version under
--    `ORDER BY activated_at DESC`. The same gap exists in
--    `find_active_stock_policy_version` (crates/db/src/stock.rs, a different
--    branch) and predates this function, but this function copied the
--    unguarded query rather than fixing it.
--
-- 3. When no active stock policy version exists at all, the function
--    silently published a valid, empty snapshot (no error, no signal to the
--    caller that nothing was actually computed). It now raises instead.
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
    v_active_stock_policy_version_id bigint;
BEGIN
    IF p_formula_version IS NULL OR p_formula_version = '' THEN
        RAISE EXCEPTION 'formula version is required' USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended('collection_scarcity_snapshot_publish', 0)
    );

    SELECT id INTO v_active_stock_policy_version_id
      FROM public.stock_policy_versions
     WHERE activated_at IS NOT NULL
       AND activated_at <= clock_timestamp()
       AND retired_at IS NULL
     ORDER BY activated_at DESC, id DESC
     LIMIT 1;
    IF v_active_stock_policy_version_id IS NULL THEN
        RAISE EXCEPTION 'no active stock policy version' USING ERRCODE = '23514';
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
           AND band.stock_policy_version_id = v_active_stock_policy_version_id
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
