-- Structural checks for the canonical outcome projection.  Fixture-heavy
-- economic scenarios remain in the Rust integration suite; these checks make
-- sure the runtime can only reach the narrow owner-bound function.
DO $block$
DECLARE
    function_exists boolean;
    runtime_can_execute boolean;
BEGIN
    SELECT to_regprocedure('public.read_quote_canonical_outcomes(bigint,uuid,bigint[])') IS NOT NULL
      INTO function_exists;
    IF NOT function_exists THEN
        RAISE EXCEPTION 'canonical outcome projection is missing';
    END IF;
    SELECT has_function_privilege('contracter_runtime',
        'public.read_quote_canonical_outcomes(bigint,uuid,bigint[])', 'EXECUTE')
      INTO runtime_can_execute;
    IF to_regrole('contracter_runtime') IS NOT NULL AND NOT runtime_can_execute THEN
        RAISE EXCEPTION 'runtime cannot execute canonical outcome projection';
    END IF;
END
$block$;
