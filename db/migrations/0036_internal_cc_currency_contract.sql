-- Internal economy values are Contracter Coins (CC), represented as integer
-- micro-CC. Migration 0033 remains byte-identical; this migration corrects its
-- human-facing diagnostics without changing the established boundaries.
CREATE OR REPLACE FUNCTION assert_quote_value_limits(
    p_input_price_microcredits bigint DEFAULT NULL,
    p_buyback_microcredits bigint DEFAULT NULL
)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
BEGIN
    IF p_input_price_microcredits IS NOT NULL AND p_input_price_microcredits < 20000000 THEN
        RAISE EXCEPTION 'quote input item is below the 20 CC minimum'
            USING ERRCODE = '23514';
    END IF;
    IF p_buyback_microcredits IS NOT NULL AND
       (p_buyback_microcredits < 0 OR p_buyback_microcredits > 15000000000) THEN
        RAISE EXCEPTION 'quote outcome exceeds the 15000 CC absolute cap'
            USING ERRCODE = '23514';
    END IF;
END;
$function$;

REVOKE ALL ON FUNCTION assert_quote_value_limits(bigint,bigint) FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE ALL ON FUNCTION assert_quote_value_limits(bigint,bigint)
            FROM contracter_runtime;
    END IF;
END;
$block$;
