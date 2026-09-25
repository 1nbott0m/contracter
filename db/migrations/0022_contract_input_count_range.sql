-- Existing deployments have the original `finalize_contract` function from
-- 0004. PostgreSQL cannot patch a PL/pgSQL function body in place, so obtain
-- its canonical definition, replace only the cardinality invariant, and
-- install that exact definition again. The guards fail migration explicitly
-- if a prior manual change made the expected old body unavailable.
DO $block$
DECLARE
    definition text;
    updated_definition text;
BEGIN
    SELECT pg_get_functiondef(
        'public.finalize_contract(bigint, bigint[], uuid)'::regprocedure
    ) INTO definition;

    updated_definition := replace(
        definition,
        'cardinality(p_locked_input_ids) <> 10',
        'cardinality(p_locked_input_ids) NOT BETWEEN 4 AND 10'
    );
    updated_definition := replace(
        updated_definition,
        'requested_input(input_id)) <> 10',
        'requested_input(input_id)) NOT BETWEEN 4 AND 10'
    );
    updated_definition := replace(
        updated_definition,
        '(SELECT count(*) FROM public.quote_inputs WHERE quote_id = p_quote_id) <> 10',
        '(SELECT count(*) FROM public.quote_inputs WHERE quote_id = p_quote_id) NOT BETWEEN 4 AND 10'
    );
    updated_definition := replace(
        updated_definition,
        'contract finalization requires exactly ten unique inputs',
        'contract finalization requires between 4 and 10 unique inputs'
    );

    IF updated_definition = definition AND
       position('cardinality(p_locked_input_ids) NOT BETWEEN 4 AND 10' IN definition) = 0 THEN
        RAISE EXCEPTION 'cannot upgrade finalize_contract cardinality invariant safely';
    END IF;

    IF updated_definition <> definition THEN
        EXECUTE updated_definition;
    END IF;
END;
$block$;
