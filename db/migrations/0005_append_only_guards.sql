CREATE OR REPLACE FUNCTION reject_append_only_mutation()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
BEGIN
    RAISE EXCEPTION '% is append-only; write a compensating event', TG_TABLE_NAME
        USING ERRCODE = '55000';
END;
$function$;

CREATE OR REPLACE FUNCTION require_journal_owner_insert()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
DECLARE
    relation_owner name;
BEGIN
    SELECT pg_get_userbyid(relation.relowner)
      INTO relation_owner
      FROM pg_class AS relation
     WHERE relation.oid = TG_RELID;

    -- A SECURITY DEFINER writer runs as the table owner. A runtime role cannot
    -- bypass this check with an arbitrary custom session setting.
    IF current_user <> relation_owner THEN
        RAISE EXCEPTION 'direct INSERT into % is forbidden', TG_TABLE_NAME
            USING ERRCODE = '42501';
    END IF;

    RETURN NULL;
END;
$function$;

DO $block$
DECLARE
    relation_name text;
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
        'ledger_transactions',
        'ledger_postings',
        'inventory_transfer_events',
        'seed_commitments',
        'seed_commitment_events',
        'seed_revelation_events',
        'critical_actions',
        'critical_action_approval_events',
        'critical_action_execution_events',
        'credit_adjustment_events',
        'quote_inputs',
        'quote_outcomes',
        'contract_inputs',
        'contract_outcomes',
        'quote_acceptance_events',
        'sale_evidence',
        'valuation_snapshot_items'
    ]
    LOOP
        EXECUTE format(
            'DROP TRIGGER IF EXISTS journal_no_update_delete ON public.%I',
            relation_name
        );
        EXECUTE format(
            'CREATE TRIGGER journal_no_update_delete '
            'BEFORE UPDATE OR DELETE ON public.%I FOR EACH STATEMENT '
            'EXECUTE FUNCTION public.reject_append_only_mutation()',
            relation_name
        );
    END LOOP;
END;
$block$;

DO $block$
DECLARE
    relation_name text;
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
        'ledger_transactions',
        'ledger_postings',
        'inventory_transfer_events',
        'seed_commitment_events',
        'seed_revelation_events',
        'critical_action_approval_events',
        'critical_action_execution_events',
        'credit_adjustment_events',
        'quote_inputs',
        'quote_outcomes',
        'contract_inputs',
        'contract_outcomes',
        'quote_acceptance_events',
        'sale_evidence',
        'valuation_snapshot_items'
    ]
    LOOP
        EXECUTE format(
            'DROP TRIGGER IF EXISTS journal_owner_insert_only ON public.%I',
            relation_name
        );
        EXECUTE format(
            'CREATE TRIGGER journal_owner_insert_only '
            'BEFORE INSERT ON public.%I FOR EACH STATEMENT '
            'EXECUTE FUNCTION public.require_journal_owner_insert()',
            relation_name
        );
    END LOOP;
END;
$block$;

CREATE OR REPLACE FUNCTION approve_critical_action(
    p_critical_action_id bigint,
    p_approver_admin_id bigint,
    p_payload_hash bytea
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    approval_id bigint;
BEGIN
    INSERT INTO public.critical_action_approval_events (
        critical_action_id,
        approver_admin_id,
        payload_hash
    ) VALUES (
        p_critical_action_id,
        p_approver_admin_id,
        p_payload_hash
    )
    RETURNING id INTO approval_id;

    RETURN approval_id;
END;
$function$;

REVOKE ALL ON FUNCTION public.post_ledger_transaction(text, uuid, jsonb)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION public.post_credit_adjustment(bigint, bigint, bigint, uuid, bigint)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION public.finalize_contract(bigint, bigint[], uuid)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION public.publish_valuation_snapshot(bigint)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION public.approve_critical_action(bigint, bigint, bytea)
    FROM PUBLIC;

REVOKE INSERT, UPDATE, DELETE, TRUNCATE
    ON ledger_transactions,
       ledger_postings,
       inventory_transfer_events,
       seed_commitment_events,
       seed_revelation_events,
       critical_action_approval_events,
       critical_action_execution_events,
       credit_adjustment_events,
       quote_inputs,
       quote_outcomes,
       contract_inputs,
       contract_outcomes,
       quote_acceptance_events,
       sale_evidence,
       valuation_snapshot_items
    FROM PUBLIC;

-- Managed PostgreSQL often denies CREATEROLE to migration users. Roles are
-- provisioned externally; these grants apply only when the named role exists.
-- No journal table receives direct runtime DML privileges.
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT USAGE ON SCHEMA public TO contracter_runtime;
        GRANT SELECT ON users, collections, rarities, wear_bands,
                        catalog_items, skus, inventory_items,
                        inventory_positions, current_valuations, price_halts,
                        warehouse_stock, tradeup_quotes, quote_inputs,
                        quote_outcomes, contracts, contract_inputs,
                        contract_outcomes, risk_state
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION
            public.finalize_contract(bigint, bigint[], uuid)
            TO contracter_runtime;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    ) THEN
        GRANT USAGE ON SCHEMA public TO contracter_admin_runtime;
        GRANT SELECT ON users, administrators, critical_actions,
                        critical_action_approval_events,
                        credit_adjustment_events, ledger_accounts,
                        ledger_balances, price_halts, valuation_snapshots,
                        valuation_snapshot_items, risk_state
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION
            public.post_credit_adjustment(bigint, bigint, bigint, uuid, bigint)
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION
            public.approve_critical_action(bigint, bigint, bytea)
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION
            public.publish_valuation_snapshot(bigint)
            TO contracter_admin_runtime;
    END IF;

    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_readonly') THEN
        GRANT USAGE ON SCHEMA public TO contracter_readonly;
        GRANT SELECT ON ALL TABLES IN SCHEMA public TO contracter_readonly;
    END IF;
END;
$block$;
