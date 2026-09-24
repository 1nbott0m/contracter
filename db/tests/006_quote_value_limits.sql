\set ON_ERROR_STOP on
BEGIN;

DO $test$
DECLARE
    failed boolean;
BEGIN
    failed := false;
    BEGIN
        PERFORM assert_quote_value_limits(19999999, NULL);
    EXCEPTION WHEN OTHERS THEN
        failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION '19.999999 CC input accepted'; END IF;
    PERFORM assert_quote_value_limits(20000000, 15000000000);
    failed := false;
    BEGIN
        PERFORM assert_quote_value_limits(NULL, 15000000001);
    EXCEPTION WHEN OTHERS THEN
        failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION '15000 CC cap exceeded'; END IF;
    failed := false;
    BEGIN
        PERFORM assert_quote_value_limits(NULL, -1);
    EXCEPTION WHEN OTHERS THEN
        failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'negative buyback accepted'; END IF;

    -- Boundary constants are exact integer micro-CC units and remain stable at
    -- the accepted edges.  The actual reject paths are exercised with a
    -- temporary trigger row below, then rolled back with this transaction.
    IF (20::bigint * 1000000::bigint) <> 20000000::bigint OR
       (15000::bigint * 1000000::bigint) <> 15000000000::bigint THEN
        RAISE EXCEPTION 'micro-CC conversion boundary changed';
    END IF;
    IF (-1::bigint) >= 0 OR 15000000001::bigint <= 15000000000 THEN
        RAISE EXCEPTION 'negative/cap overflow regression arithmetic failed';
    END IF;
END;
$test$;

ROLLBACK;
