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
CREATE OR REPLACE FUNCTION publish_collection_scarcity_snapshot(p_snapshot_id bigint)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    snapshot_row public.collection_scarcity_snapshots%ROWTYPE;
BEGIN
    SELECT * INTO snapshot_row
      FROM public.collection_scarcity_snapshots
     WHERE id = p_snapshot_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'collection scarcity snapshot does not exist'
            USING ERRCODE = '23503';
    END IF;
    IF snapshot_row.published_at IS NOT NULL THEN
        RETURN;
    END IF;

    UPDATE public.collection_scarcity_snapshots
       SET published_at = clock_timestamp()
     WHERE id = p_snapshot_id;

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
     WHERE item.snapshot_id = p_snapshot_id
    ON CONFLICT (collection_id) DO UPDATE
    SET snapshot_id = EXCLUDED.snapshot_id,
        snapshot_item_id = EXCLUDED.snapshot_item_id,
        weight_multiplier_numerator = EXCLUDED.weight_multiplier_numerator,
        weight_multiplier_denominator = EXCLUDED.weight_multiplier_denominator,
        updated_at = EXCLUDED.updated_at;
END;
$function$;

REVOKE ALL ON FUNCTION publish_collection_scarcity_snapshot(bigint) FROM PUBLIC;

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
