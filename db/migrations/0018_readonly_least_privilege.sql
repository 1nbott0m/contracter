-- `contracter_readonly` was initially intended for reporting, but migration
-- 0005 granted it SELECT on every public table. That included password and
-- session-token hashes, administrator recovery material, and server seeds.
-- Migrations are immutable once SQLx has recorded their checksum, so narrow
-- the existing grant here instead of changing 0005.
--
-- The allowlist contains only catalog, pricing, stock, risk, scarcity, and
-- public-verification data. Account, authentication, administration, ledger,
-- individual inventory, quote/contract, and unrevealed-seed relations remain
-- deliberately absent.
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_readonly') THEN
        REVOKE SELECT ON ALL TABLES IN SCHEMA public FROM contracter_readonly;

        GRANT USAGE ON SCHEMA public TO contracter_readonly;
        GRANT SELECT ON
            collections,
            rarities,
            wear_bands,
            catalog_items,
            skus,
            price_sources,
            current_valuations,
            price_halts,
            valuation_snapshots,
            valuation_snapshot_items,
            stock_policy_versions,
            stock_policy_bands,
            warehouse_stock,
            risk_policy_versions,
            risk_state,
            collection_scarcity_snapshots,
            collection_scarcity_snapshot_items,
            current_collection_scarcity,
            quote_signing_keys,
            seed_commitments,
            seed_daily_roots
            TO contracter_readonly;
    END IF;
END;
$block$;
