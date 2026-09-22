-- Existing deployments have the original `finalize_contract` function from
-- 0004. PostgreSQL cannot patch a PL/pgSQL function body in place, so obtain
-- its canonical definition, replace only the cardinality invariant, and
-- install that exact definition again. The guards fail migration explicitly
-- if a prior manual change made the expected old body unavailable.
DO $block$
DECLARE
    definition text;
    old_predicate constant text := $text$
cardinality(p_locked_input_ids) <> 10 OR
       (SELECT count(DISTINCT requested_input.input_id)
          FROM unnest(p_locked_input_ids) AS requested_input(input_id)) <> 10$text$;
    new_predicate constant text := $text$
cardinality(p_locked_input_ids) NOT BETWEEN 4 AND 10 OR
       (SELECT count(DISTINCT requested_input.input_id)
          FROM unnest(p_locked_input_ids) AS requested_input(input_id)) NOT BETWEEN 4 AND 10$text$;
    old_quote_count constant text :=
        '(SELECT count(*) FROM public.quote_inputs WHERE quote_id = p_quote_id) <> 10 OR';
    new_quote_count constant text :=
        '(SELECT count(*) FROM public.quote_inputs WHERE quote_id = p_quote_id) NOT BETWEEN 4 AND 10 OR';
BEGIN
    SELECT pg_get_functiondef(
        'public.finalize_contract(bigint, bigint[], uuid)'::regprocedure
    ) INTO definition;

    IF position('cardinality(p_locked_input_ids) NOT BETWEEN 4 AND 10' IN definition) > 0 AND
       position('contract finalization requires between 4 and 10 unique inputs' IN definition) > 0 THEN
        RETURN;
    END IF;

    IF position(old_predicate IN definition) = 0 OR
       position('contract finalization requires exactly ten unique inputs' IN definition) = 0 THEN
        RAISE EXCEPTION 'cannot upgrade finalize_contract cardinality invariant safely';
    END IF;

    definition := replace(definition, old_predicate, new_predicate);
    definition := replace(definition, old_quote_count, new_quote_count);
    definition := replace(
        definition,
        'contract finalization requires exactly ten unique inputs',
        'contract finalization requires between 4 and 10 unique inputs'
    );
    EXECUTE definition;
END;
$block$;
