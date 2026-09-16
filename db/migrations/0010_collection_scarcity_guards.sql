-- 0009 introduced the scarcity-snapshot tables and publish function without
-- two protections every analogous table already has: the append-only guard
-- that `valuation_snapshot_items` receives in 0005, and a grant letting any
-- role actually call the publish function (compare `publish_valuation_snapshot`,
-- granted to `contracter_admin_runtime` in 0005). This migration closes both
-- gaps without touching 0001-0009.

DROP TRIGGER IF EXISTS journal_no_update_delete
    ON collection_scarcity_snapshot_items;
CREATE TRIGGER journal_no_update_delete
BEFORE UPDATE OR DELETE ON collection_scarcity_snapshot_items FOR EACH STATEMENT
EXECUTE FUNCTION reject_append_only_mutation();

DROP TRIGGER IF EXISTS journal_owner_insert_only
    ON collection_scarcity_snapshot_items;
CREATE TRIGGER journal_owner_insert_only
BEFORE INSERT ON collection_scarcity_snapshot_items FOR EACH STATEMENT
EXECUTE FUNCTION require_journal_owner_insert();

REVOKE INSERT, UPDATE, DELETE, TRUNCATE
    ON collection_scarcity_snapshot_items
    FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    ) THEN
        GRANT SELECT ON collection_scarcity_snapshots,
                        collection_scarcity_snapshot_items,
                        current_collection_scarcity
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION
            public.publish_collection_scarcity_snapshot(bigint)
            TO contracter_admin_runtime;
    END IF;
END;
$block$;
