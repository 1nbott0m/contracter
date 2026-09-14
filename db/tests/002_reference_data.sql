\set ON_ERROR_STOP on

BEGIN;

DO $test$
DECLARE
    actual_targets integer[];
BEGIN
    IF (SELECT count(*) FROM rarities) < 6 THEN
        RAISE EXCEPTION 'reference test failed: rarity rows are missing';
    END IF;

    IF (SELECT count(*) FROM wear_bands) <> 5 OR
       NOT EXISTS (
           SELECT 1 FROM wear_bands
           WHERE code = 'battle_scarred'
             AND lower_bound = 0.45000000
             AND upper_bound = 1.00000000
             AND includes_upper_bound
       ) THEN
        RAISE EXCEPTION 'reference test failed: wear bands do not cover the approved model';
    END IF;

    IF (SELECT count(*) FROM collections WHERE slug IN (
        'control', 'ancient', 'havoc', '2021-dust-2', '2021-mirage',
        '2021-vertigo', 'canals', 'norse', '2021-train', '2018-nuke'
    )) <> 10 THEN
        RAISE EXCEPTION 'reference test failed: selected collection rows are missing';
    END IF;

    SELECT array_agg(band.target_units ORDER BY rarity.rank)
      INTO actual_targets
      FROM stock_policy_versions AS policy
      JOIN stock_policy_bands AS band
        ON band.stock_policy_version_id = policy.id
      JOIN rarities AS rarity ON rarity.code = band.rarity_code
     WHERE policy.version = 1;

    IF actual_targets IS DISTINCT FROM ARRAY[167, 125, 83, 42, 17, 7] THEN
        RAISE EXCEPTION 'reference test failed: stock targets are %', actual_targets;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM risk_policy_versions
        WHERE version = 1
          AND minimum_coverage_ratio = 1.25000000
          AND maximum_quote_reserve_ratio = 0.01000000
          AND maximum_item_liability_ratio = 0.10000000
          AND maximum_collection_liability_ratio = 0.25000000
          AND activated_at IS NULL
    ) THEN
        RAISE EXCEPTION 'reference test failed: inactive risk policy v1 is invalid';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM price_sources
        WHERE code = 'market_csgo' AND NOT enabled
    ) THEN
        RAISE EXCEPTION 'reference test failed: uncalibrated price source must be disabled';
    END IF;
END;
$test$;

SELECT '002_reference_data: ok' AS result;

ROLLBACK;
