BEGIN;

DO $block$
DECLARE
    history_id bigint;
    visible_count integer;
BEGIN
    SELECT record_risk_state_history('verification', '{"source":"sql-test"}'::jsonb)
      INTO history_id;
    IF history_id IS NULL THEN
        RAISE EXCEPTION 'risk history writer returned NULL';
    END IF;

    SELECT count(*) INTO visible_count
      FROM read_risk_state_history(10)
     WHERE id = history_id AND reason = 'verification';
    IF visible_count <> 1 THEN
        RAISE EXCEPTION 'risk history read boundary did not return inserted event';
    END IF;

    BEGIN
        UPDATE risk_state_history SET reason = 'tampered' WHERE id = history_id;
        RAISE EXCEPTION 'risk history update unexpectedly succeeded';
    EXCEPTION WHEN SQLSTATE '55000' THEN
        NULL;
    END;

    PERFORM set_config('role', 'contracter_runtime', true);
    BEGIN
        INSERT INTO risk_state_history (
            risk_state_version, liquid_reserve_microcredits,
            stressed_liability_microcredits, outstanding_quote_exposure_microcredits,
            reason
        ) VALUES (0, 0, 0, 0, 'direct');
        RAISE EXCEPTION 'direct risk history insert unexpectedly succeeded';
    EXCEPTION WHEN SQLSTATE '42501' THEN
        NULL;
    END;
END;
$block$;

ROLLBACK;
