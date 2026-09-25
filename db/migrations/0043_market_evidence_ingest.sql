-- Server-side Market.CSGO evidence boundary.
-- The HTTP runtime never writes these tables directly. A separate importer
-- calls these SECURITY DEFINER functions with normalized, already-authenticated
-- source rows. No valuation is published until each SKU has >= 20 valid sales.

INSERT INTO currencies (code, minor_unit_scale)
VALUES ('RUB', 2)
ON CONFLICT (code) DO NOTHING;

CREATE OR REPLACE FUNCTION ingest_market_sale_evidence(
    p_source_code text,
    p_rows jsonb,
    p_rub_to_cc_rate numeric
) RETURNS integer
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    row_data jsonb;
    v_sku_id bigint;
    v_inserted integer := 0;
    v_rub numeric;
    v_cc_micro bigint;
    v_event_key text;
    v_source_at timestamptz;
    v_digest bytea;
BEGIN
    IF p_source_code IS NULL OR p_source_code = '' OR
       NOT EXISTS (SELECT 1 FROM public.price_sources WHERE code = p_source_code AND enabled) THEN
        RAISE EXCEPTION 'price source is not enabled' USING ERRCODE = '42501';
    END IF;
    IF p_rows IS NULL OR jsonb_typeof(p_rows) <> 'array' THEN
        RAISE EXCEPTION 'sale rows must be a JSON array' USING ERRCODE = '22P02';
    END IF;
    IF p_rub_to_cc_rate IS NULL OR p_rub_to_cc_rate <= 0 THEN
        RAISE EXCEPTION 'RUB to CC rate must be positive' USING ERRCODE = '23514';
    END IF;

    FOR row_data IN SELECT value FROM jsonb_array_elements(p_rows)
    LOOP
        IF jsonb_typeof(row_data) <> 'object' OR
           row_data->>'sku_public_id' IS NULL OR
           row_data->>'external_event_key' IS NULL OR
           row_data->>'source_timestamp' IS NULL OR
           row_data->>'price_rub' IS NULL THEN
            RAISE EXCEPTION 'sale row is missing required fields' USING ERRCODE = '22P02';
        END IF;
        SELECT id INTO v_sku_id FROM public.skus
         WHERE public_id = (row_data->>'sku_public_id')::uuid AND enabled;
        IF NOT FOUND THEN
            CONTINUE;
        END IF;
        v_event_key := left(row_data->>'external_event_key', 500);
        v_rub := (row_data->>'price_rub')::numeric;
        v_source_at := (row_data->>'source_timestamp')::timestamptz;
        IF v_rub <= 0 OR v_source_at > clock_timestamp() THEN
            CONTINUE;
        END IF;
        IF v_rub * p_rub_to_cc_rate * 1000000 > 9223372036854775807::numeric THEN
            RAISE EXCEPTION 'sale price exceeds microcredit storage bound' USING ERRCODE = '22003';
        END IF;
        v_cc_micro := round(v_rub * p_rub_to_cc_rate * 1000000)::bigint;
        IF v_cc_micro <= 0 THEN CONTINUE; END IF;
        v_digest := public.digest(convert_to(row_data::text, 'UTF8'), 'sha256');
        INSERT INTO public.sale_evidence (
            source_code, external_event_key, sku_id, variant_code,
            currency_code, gross_microcredits, source_timestamp,
            validity_reason_code, normalized_payload_hash
        ) VALUES (
            p_source_code, v_event_key, v_sku_id, 'normal', 'RUB',
            v_cc_micro, v_source_at, 'valid', v_digest
        ) ON CONFLICT (source_code, external_event_key) DO NOTHING;
        IF FOUND THEN v_inserted := v_inserted + 1; END IF;
    END LOOP;
    RETURN v_inserted;
END;
$function$;

CREATE OR REPLACE FUNCTION build_market_valuation_snapshot(
    p_source_code text,
    p_formula_version text,
    p_window_days smallint DEFAULT 30
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    v_snapshot_id bigint;
BEGIN
    IF p_window_days NOT IN (7, 30) OR p_formula_version IS NULL OR p_formula_version = '' THEN
        RAISE EXCEPTION 'invalid valuation snapshot parameters' USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM public.price_sources WHERE code = p_source_code AND enabled) THEN
        RAISE EXCEPTION 'price source is not enabled' USING ERRCODE = '42501';
    END IF;
    INSERT INTO public.valuation_snapshots (formula_version, snapshot_at)
    VALUES (p_formula_version, clock_timestamp()) RETURNING id INTO v_snapshot_id;

    INSERT INTO public.valuation_snapshot_items (
        snapshot_id, sku_id, verified_price_microcredits, source_code,
        window_days, valid_sale_count, evidence_cutoff_at, evidence_digest
    )
    WITH eligible AS (
        SELECT e.*, count(*) OVER (PARTITION BY e.sku_id) AS sale_count,
               row_number() OVER (PARTITION BY e.sku_id ORDER BY e.gross_microcredits, e.id) AS rn
          FROM public.sale_evidence e
         WHERE e.source_code = p_source_code
           AND e.validity_reason_code = 'valid'
           AND e.source_timestamp >= clock_timestamp() - make_interval(days => p_window_days::int)
    ), trimmed AS (
        SELECT sku_id, sale_count,
               avg(gross_microcredits)::numeric AS mean_price,
               max(source_timestamp) AS cutoff,
               public.digest(convert_to(string_agg(id::text, ',' ORDER BY id), 'UTF8'), 'sha256') AS evidence_digest
          FROM eligible
         WHERE sale_count >= 20
           AND rn > floor(sale_count * 0.10)
           AND rn <= sale_count - floor(sale_count * 0.10)
         GROUP BY sku_id, sale_count
    )
    SELECT v_snapshot_id, sku_id, round(mean_price)::bigint, p_source_code,
           p_window_days, sale_count, cutoff, evidence_digest
      FROM trimmed;

    IF NOT EXISTS (SELECT 1 FROM public.valuation_snapshot_items WHERE snapshot_id = v_snapshot_id) THEN
        RAISE EXCEPTION 'not enough valid sale evidence to publish a valuation snapshot'
            USING ERRCODE = '23514';
    END IF;
    PERFORM public.publish_valuation_snapshot(v_snapshot_id);
    RETURN v_snapshot_id;
END;
$function$;

REVOKE ALL ON FUNCTION ingest_market_sale_evidence(text, jsonb, numeric) FROM PUBLIC;
REVOKE ALL ON FUNCTION build_market_valuation_snapshot(text, text, smallint) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime') THEN
        GRANT EXECUTE ON FUNCTION ingest_market_sale_evidence(text, jsonb, numeric)
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION build_market_valuation_snapshot(text, text, smallint)
            TO contracter_admin_runtime;
    END IF;
END;
$block$;
