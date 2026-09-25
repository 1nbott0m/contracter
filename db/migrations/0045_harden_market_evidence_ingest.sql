-- Follow-up hardening for the published market evidence functions.
-- Migration 0043 is immutable because production has already recorded its checksum.

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
        IF NOT FOUND THEN CONTINUE; END IF;
        v_event_key := left(row_data->>'external_event_key', 500);
        v_rub := (row_data->>'price_rub')::numeric;
        v_source_at := (row_data->>'source_timestamp')::timestamptz;
        IF v_rub <= 0 OR v_source_at > clock_timestamp() THEN CONTINUE; END IF;
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

REVOKE ALL ON FUNCTION ingest_market_sale_evidence(text, jsonb, numeric) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime') THEN
        GRANT EXECUTE ON FUNCTION ingest_market_sale_evidence(text, jsonb, numeric)
            TO contracter_admin_runtime;
    END IF;
END;
$block$;
