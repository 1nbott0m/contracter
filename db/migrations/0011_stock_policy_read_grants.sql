DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT SELECT ON stock_policy_versions,
                        stock_policy_bands
            TO contracter_runtime;
    END IF;
END;
$block$;
