CREATE OR REPLACE FUNCTION post_credit_adjustment(
    p_initiator_admin_id bigint,
    p_target_user_id bigint,
    p_amount_microcredits bigint,
    p_execution_key uuid,
    p_critical_action_id bigint
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    system_account_id bigint;
    target_account_id bigint;
    existing_adjustment public.credit_adjustment_events%ROWTYPE;
    rolling_amount numeric;
    approval_required boolean;
    action_row public.critical_actions%ROWTYPE;
    expected_payload jsonb;
    transaction_id bigint;
BEGIN
    IF p_amount_microcredits IS NULL OR p_amount_microcredits = 0 OR
       p_amount_microcredits = '-9223372036854775808'::bigint OR
       p_execution_key IS NULL THEN
        RAISE EXCEPTION 'invalid credit adjustment request'
            USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended(p_execution_key::text, 1));

    SELECT adjustment.*
      INTO existing_adjustment
      FROM public.credit_adjustment_events AS adjustment
     WHERE adjustment.execution_key = p_execution_key;
    IF FOUND THEN
        IF existing_adjustment.initiator_admin_id IS DISTINCT FROM
               p_initiator_admin_id OR
           existing_adjustment.target_user_id IS DISTINCT FROM
               p_target_user_id OR
           existing_adjustment.amount_microcredits IS DISTINCT FROM
               p_amount_microcredits OR
           existing_adjustment.critical_action_id IS DISTINCT FROM
               p_critical_action_id THEN
            RAISE EXCEPTION 'execution key was reused with another request'
                USING ERRCODE = '23505';
        END IF;
        RETURN existing_adjustment.ledger_transaction_id;
    END IF;

    PERFORM 1
      FROM public.administrators AS administrator
     WHERE administrator.id = p_initiator_admin_id
       AND administrator.is_active
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'initiator must be an active administrator'
            USING ERRCODE = '42501';
    END IF;

    SELECT account.id INTO system_account_id
      FROM public.ledger_accounts AS account
     WHERE account.kind_code = 'system_treasury'
       AND account.owner_user_id IS NULL
       AND account.closed_at IS NULL;

    SELECT account.id INTO target_account_id
      FROM public.ledger_accounts AS account
     WHERE account.kind_code = 'user_credit'
       AND account.owner_user_id = p_target_user_id
       AND account.closed_at IS NULL;

    IF system_account_id IS NULL OR target_account_id IS NULL THEN
        RAISE EXCEPTION 'required credit ledger account is missing'
            USING ERRCODE = '23503';
    END IF;

    PERFORM account.id
      FROM public.ledger_accounts AS account
     WHERE account.id IN (system_account_id, target_account_id)
     ORDER BY account.id
     FOR UPDATE;

    SELECT COALESCE(sum(abs(adjustment.amount_microcredits::numeric)), 0)
      INTO rolling_amount
      FROM public.credit_adjustment_events AS adjustment
     WHERE adjustment.initiator_admin_id = p_initiator_admin_id
       AND adjustment.target_user_id = p_target_user_id
       AND adjustment.occurred_at >= clock_timestamp() - interval '24 hours';

    approval_required := abs(p_amount_microcredits::numeric) > 100000000 OR
                         rolling_amount + abs(p_amount_microcredits::numeric) > 100000000;

    IF approval_required THEN
        IF p_critical_action_id IS NULL THEN
            RAISE EXCEPTION 'dual approval is required for this credit adjustment'
                USING ERRCODE = '42501';
        END IF;

        SELECT * INTO action_row
          FROM public.critical_actions AS action
         WHERE action.id = p_critical_action_id
         FOR UPDATE;

        expected_payload := jsonb_build_object(
            'target_user_id', p_target_user_id,
            'amount_microcredits', p_amount_microcredits,
            'execution_key', p_execution_key
        );

        IF NOT FOUND OR action_row.action_type_code <> 'credit_adjustment' OR
           action_row.proposer_admin_id <> p_initiator_admin_id OR
           action_row.payload <> expected_payload OR
           action_row.payload_hash <> public.digest(
               convert_to(expected_payload::text, 'UTF8'), 'sha256'
           ) OR
           clock_timestamp() > action_row.expires_at OR
           clock_timestamp() > action_row.requested_at + interval '24 hours' THEN
            RAISE EXCEPTION 'critical action does not authorize this adjustment'
                USING ERRCODE = '42501';
        END IF;

        IF NOT EXISTS (
            SELECT 1
              FROM public.critical_action_approval_events AS approval
              JOIN public.administrators AS approver
                ON approver.id = approval.approver_admin_id
             WHERE approval.critical_action_id = action_row.id
               AND approval.payload_hash = action_row.payload_hash
               AND approval.approver_admin_id <> action_row.proposer_admin_id
               AND approver.is_active
               AND approval.approved_at <= action_row.expires_at
               AND approval.approved_at <= action_row.requested_at + interval '24 hours'
        ) OR EXISTS (
            SELECT 1
              FROM public.critical_action_execution_events AS execution
             WHERE execution.critical_action_id = action_row.id
        ) THEN
            RAISE EXCEPTION 'valid unused second approval is required'
                USING ERRCODE = '42501';
        END IF;
    ELSIF p_critical_action_id IS NOT NULL THEN
        RAISE EXCEPTION 'unexpected critical action for unprivileged adjustment'
            USING ERRCODE = '23514';
    END IF;

    transaction_id := public.post_ledger_transaction(
        'credit_adjustment',
        p_execution_key,
        jsonb_build_array(
            jsonb_build_object(
                'account_id', system_account_id,
                'amount_microcredits', -p_amount_microcredits
            ),
            jsonb_build_object(
                'account_id', target_account_id,
                'amount_microcredits', p_amount_microcredits
            )
        )
    );

    INSERT INTO public.credit_adjustment_events (
        initiator_admin_id,
        target_user_id,
        amount_microcredits,
        execution_key,
        critical_action_id,
        ledger_transaction_id
    ) VALUES (
        p_initiator_admin_id,
        p_target_user_id,
        p_amount_microcredits,
        p_execution_key,
        p_critical_action_id,
        transaction_id
    );

    IF approval_required THEN
        INSERT INTO public.critical_action_execution_events (
            critical_action_id,
            execution_key,
            payload_hash,
            result_reference
        ) VALUES (
            action_row.id,
            p_execution_key,
            action_row.payload_hash,
            'ledger_transaction:' || transaction_id::text
        );
    END IF;

    RETURN transaction_id;
END;
$function$;

REVOKE ALL ON FUNCTION post_credit_adjustment(bigint, bigint, bigint, uuid, bigint)
    FROM PUBLIC;
