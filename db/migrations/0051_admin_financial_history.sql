CREATE OR REPLACE FUNCTION list_admin_ledger_transactions(p_admin_public_id uuid)
RETURNS TABLE(transaction_id uuid, user_id uuid, operation_kind text, amount_microcredits bigint, occurred_at timestamptz)
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT t.public_id, u.public_id, t.operation_kind, p.amount_microcredits, t.created_at
   FROM ledger_transactions t
   JOIN ledger_postings p ON p.ledger_transaction_id=t.id
   JOIN ledger_accounts a ON a.id=p.account_id
   LEFT JOIN users u ON u.id=a.owner_user_id
  WHERE EXISTS (SELECT 1 FROM administrators ad JOIN users au ON au.id=ad.user_id
                 WHERE au.public_id=p_admin_public_id AND ad.is_active AND ad.deactivated_at IS NULL)
  ORDER BY t.created_at DESC, t.id DESC LIMIT 500;
$$;

CREATE OR REPLACE FUNCTION list_admin_market_purchases(p_admin_public_id uuid)
RETURNS TABLE(purchase_id uuid, user_id uuid, sku_id uuid, inventory_item_id uuid, amount_microcredits bigint, occurred_at timestamptz)
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT e.public_id, u.public_id, s.public_id, i.public_id, e.amount_microcredits, e.occurred_at
   FROM market_purchase_events e
   JOIN users u ON u.id=e.user_id
   JOIN skus s ON s.id=e.sku_id
   JOIN inventory_items i ON i.id=e.inventory_item_id
  WHERE EXISTS (SELECT 1 FROM administrators ad JOIN users au ON au.id=ad.user_id
                 WHERE au.public_id=p_admin_public_id AND ad.is_active AND ad.deactivated_at IS NULL)
  ORDER BY e.occurred_at DESC, e.id DESC LIMIT 500;
$$;

REVOKE ALL ON FUNCTION list_admin_ledger_transactions(uuid), list_admin_market_purchases(uuid) FROM PUBLIC;
DO $block$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
    GRANT EXECUTE ON FUNCTION list_admin_ledger_transactions(uuid), list_admin_market_purchases(uuid) TO contracter_runtime;
  END IF;
END $block$;
