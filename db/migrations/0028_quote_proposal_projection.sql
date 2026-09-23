CREATE OR REPLACE FUNCTION public.read_quote_proposal_projection(
    p_user_id bigint, p_allocation_public_id uuid, p_inventory_item_public_ids uuid[]
)
RETURNS TABLE (
    allocation_public_id uuid, commitment_hash bytea, seed_nonce bytea,
    seed_ciphertext bytea, allocation_expires_at timestamptz,
    inventory_item_id bigint, inventory_item_public_id uuid, sku_id bigint,
    sku_public_id uuid, canonical_float numeric, locked_position_version bigint,
    catalog_item_id bigint, collection_id bigint, rarity_code text,
    min_float numeric, max_float numeric, valuation_snapshot_id bigint,
    valuation_snapshot_item_id bigint, verified_price_microcredits bigint,
    stock_policy_version_id bigint, risk_policy_version_id bigint,
    signing_key_id bigint, formula_version text
)
LANGUAGE sql SECURITY DEFINER SET search_path = public, pg_temp
AS $function$
    SELECT allocation.public_id, commitment.commitment_hash, envelope.nonce,
           envelope.ciphertext, allocation.expires_at, inventory.id,
           inventory.public_id, inventory.sku_id, sku.public_id,
           inventory.canonical_float, position.version, catalog.id,
           catalog.collection_id, catalog.rarity_code, catalog.min_float,
           catalog.max_float, valuation.snapshot_id, valuation.snapshot_item_id,
           valuation.verified_price_microcredits, stock_policy.id,
           risk_state.risk_policy_version_id, signing_key.id,
           snapshot.formula_version
      FROM public.seed_allocations allocation
      JOIN public.seed_commitments commitment ON commitment.id = allocation.commitment_id
      JOIN public.seed_secret_envelopes envelope ON envelope.commitment_id = commitment.id
      JOIN public.inventory_items inventory ON inventory.public_id = ANY (p_inventory_item_public_ids)
      JOIN public.inventory_positions position ON position.inventory_item_id = inventory.id
       AND position.owner_user_id = p_user_id AND NOT position.in_warehouse
      JOIN public.skus sku ON sku.id = inventory.sku_id AND sku.enabled
      JOIN public.catalog_items catalog ON catalog.id = sku.catalog_item_id AND catalog.enabled
      JOIN public.current_valuations valuation ON valuation.sku_id = sku.id
      JOIN public.valuation_snapshots snapshot ON snapshot.id = valuation.snapshot_id
       AND snapshot.published_at IS NOT NULL
      JOIN public.risk_state risk_state ON risk_state.singleton
      JOIN public.stock_policy_versions stock_policy ON stock_policy.id = (
          SELECT id FROM public.stock_policy_versions
           WHERE activated_at IS NOT NULL AND retired_at IS NULL
           ORDER BY activated_at DESC, id DESC LIMIT 1)
      JOIN public.quote_signing_keys signing_key
        ON signing_key.activated_at <= clock_timestamp()
       AND (signing_key.retired_at IS NULL OR signing_key.retired_at > clock_timestamp())
     WHERE allocation.user_id = p_user_id
       AND allocation.public_id = p_allocation_public_id
       AND allocation.released_at IS NULL AND allocation.expires_at > clock_timestamp()
       AND valuation.snapshot_id = risk_state.valuation_snapshot_id
     ORDER BY array_position(p_inventory_item_public_ids, inventory.public_id), inventory.id;
$function$;
REVOKE ALL ON FUNCTION public.read_quote_proposal_projection(bigint, uuid, uuid[]) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION public.read_quote_proposal_projection(bigint, uuid, uuid[])
            TO contracter_runtime;
    END IF;
END;
$block$;
