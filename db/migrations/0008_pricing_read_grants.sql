DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT SELECT ON price_sources,
                        valuation_snapshots,
                        valuation_snapshot_items
            TO contracter_runtime;
    END IF;
END;
$block$;
