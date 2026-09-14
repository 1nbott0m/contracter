CREATE OR REPLACE VIEW available_user_inventory
WITH (security_barrier = true)
AS
SELECT item.id,
       item.public_id,
       item.sku_id,
       item.canonical_float,
       item.created_at,
       item.retired_at,
       position.owner_user_id,
       position.in_warehouse,
       position.version AS position_version,
       position.updated_at AS position_updated_at
FROM inventory_items AS item
JOIN inventory_positions AS position ON position.inventory_item_id = item.id
WHERE position.owner_user_id IS NOT NULL
  AND item.retired_at IS NULL
  AND NOT EXISTS (
      SELECT 1
      FROM inventory_item_locks AS item_lock
      WHERE item_lock.inventory_item_id = item.id
        AND item_lock.expires_at > clock_timestamp()
  );

CREATE OR REPLACE VIEW available_warehouse_inventory
WITH (security_barrier = true)
AS
SELECT item.id,
       item.public_id,
       item.sku_id,
       item.canonical_float,
       item.created_at,
       item.retired_at,
       position.owner_user_id,
       position.in_warehouse,
       position.version AS position_version,
       position.updated_at AS position_updated_at
FROM inventory_items AS item
JOIN inventory_positions AS position ON position.inventory_item_id = item.id
WHERE position.in_warehouse
  AND item.retired_at IS NULL
  AND NOT EXISTS (
      SELECT 1
      FROM inventory_item_locks AS item_lock
      WHERE item_lock.inventory_item_id = item.id
        AND item_lock.expires_at > clock_timestamp()
  )
  AND NOT EXISTS (
      SELECT 1
      FROM quote_candidate_reservations AS reservation
      WHERE reservation.inventory_item_id = item.id
        AND reservation.reserved_until > clock_timestamp()
  );

REVOKE ALL ON available_user_inventory, available_warehouse_inventory FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT SELECT ON available_user_inventory, available_warehouse_inventory
            TO contracter_runtime;
    END IF;
END;
$block$;
