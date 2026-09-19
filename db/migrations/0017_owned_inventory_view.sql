-- The owner's own view of their inventory.
--
-- Distinct from available_user_inventory (0007) on purpose. That view
-- answers "what can be spent" and therefore hides an item locked to a
-- quote. This one answers "what do I own", where an item silently
-- vanishing while it is reserved reads as theft -- so a locked item is
-- listed, flagged. Retired items stay out of both: those are gone rather
-- than reserved.
--
-- It is a view rather than a query in the application for the same reason
-- 0007's views are. contracter_runtime deliberately has no SELECT on
-- inventory_item_locks, and an EXISTS subquery in that role's own
-- statement is still subject to that role's privileges -- only a view
-- executes with its owner's rights. Reading the lock state through this
-- view keeps the guard table unreadable to the runtime role, which is the
-- point of the grant in 0005.
--
-- security_barrier so the planner cannot push a caller-supplied predicate
-- underneath the join and use its evaluation as a side channel.
-- Dropped first rather than replaced. `db/verify.sh` applies every
-- migration with psql and then SQLx applies them all again from zero
-- against the same database, so a migration has to be re-runnable. CREATE
-- OR REPLACE VIEW cannot change a column list, so once a later migration
-- reshapes this view, re-running this one would fail with "cannot change
-- name of view column".
DROP VIEW IF EXISTS owned_inventory;

CREATE VIEW owned_inventory
WITH (security_barrier = true)
AS
SELECT item.id,
       item.public_id,
       item.created_at,
       item.canonical_float,
       position.owner_user_id,
       EXISTS (
           SELECT 1
           FROM inventory_item_locks AS item_lock
           WHERE item_lock.inventory_item_id = item.id
             AND item_lock.expires_at > clock_timestamp()
       ) AS locked,
       skus.public_id AS sku_public_id,
       catalog_items.public_id AS catalog_item_public_id,
       collections.public_id AS collection_public_id,
       collections.display_name AS collection_display_name,
       catalog_items.stable_name,
       catalog_items.rarity_code,
       wear_bands.code AS wear_band_code,
       catalog_items.is_stattrak,
       catalog_items.is_souvenir
FROM inventory_items AS item
JOIN inventory_positions AS position ON position.inventory_item_id = item.id
JOIN skus ON skus.id = item.sku_id
JOIN catalog_items ON catalog_items.id = skus.catalog_item_id
JOIN collections ON collections.id = catalog_items.collection_id
JOIN wear_bands ON wear_bands.id = skus.wear_band_id
WHERE position.owner_user_id IS NOT NULL
  AND item.retired_at IS NULL;

REVOKE ALL ON owned_inventory FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT SELECT ON owned_inventory TO contracter_runtime;
    END IF;
END;
$block$;

-- The owner's listing pages on (created_at DESC, public_id DESC).
-- Without an index on that key every page fetches the owner's whole
-- non-retired set and sorts it, which makes keyset pagination no cheaper
-- than OFFSET for exactly the accounts where it matters most.
CREATE INDEX IF NOT EXISTS inventory_items_recent_idx
    ON inventory_items (created_at DESC, public_id DESC)
    WHERE retired_at IS NULL;
