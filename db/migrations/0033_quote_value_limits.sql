-- Hard economic limits are enforced in the database, at the last write
-- boundary.  Values are integer microcredits: one RUB is 1,000,000.
-- These checks intentionally do not depend on application configuration.
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
            RAISE EXCEPTION 'quote input item is below the 20 RUB minimum'
                USING ERRCODE = '23514';
    END IF;
    IF p_buyback_microcredits IS NOT NULL AND
       (p_buyback_microcredits < 0 OR p_buyback_microcredits > 15000000000) THEN
            RAISE EXCEPTION 'quote outcome exceeds the 15000 RUB absolute cap'
                USING ERRCODE = '23514';
    END IF;
END;
$function$;

CREATE OR REPLACE FUNCTION enforce_quote_value_limits()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog
AS $function$
DECLARE price bigint;
BEGIN
    IF TG_TABLE_NAME = 'quote_inputs' THEN
        SELECT verified_price_microcredits INTO price FROM public.valuation_snapshot_items
         WHERE id = NEW.valuation_snapshot_item_id;
        PERFORM public.assert_quote_value_limits(price, NULL);
    ELSE
        PERFORM public.assert_quote_value_limits(NULL, NEW.buyback_microcredits);
    END IF;
    RETURN NEW;
END;
$function$;

REVOKE ALL ON FUNCTION assert_quote_value_limits(bigint,bigint), enforce_quote_value_limits() FROM PUBLIC;

DROP TRIGGER IF EXISTS quote_inputs_value_limits ON public.quote_inputs;
CREATE CONSTRAINT TRIGGER quote_inputs_value_limits
AFTER INSERT OR UPDATE ON public.quote_inputs
DEFERRABLE INITIALLY IMMEDIATE
FOR EACH ROW EXECUTE FUNCTION enforce_quote_value_limits();

DROP TRIGGER IF EXISTS quote_outcomes_value_limits ON public.quote_outcomes;
CREATE TRIGGER quote_outcomes_value_limits
BEFORE INSERT OR UPDATE OF buyback_microcredits ON public.quote_outcomes
FOR EACH ROW EXECUTE FUNCTION enforce_quote_value_limits();

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE ALL ON FUNCTION assert_quote_value_limits(bigint,bigint), enforce_quote_value_limits() FROM contracter_runtime;
    END IF;
END;
$block$;
