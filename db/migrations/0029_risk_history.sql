-- Append-only audit trail for risk-state and policy changes.  This is an
-- internal operational journal, not a user-facing balance or quote source.
CREATE TABLE IF NOT EXISTS risk_state_history (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    risk_state_version bigint NOT NULL,
    valuation_snapshot_id bigint REFERENCES valuation_snapshots(id),
    risk_policy_version_id bigint REFERENCES risk_policy_versions(id),
    liquid_reserve_microcredits bigint NOT NULL,
    stressed_liability_microcredits bigint NOT NULL,
    outstanding_quote_exposure_microcredits bigint NOT NULL,
    reason text NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (risk_state_version >= 0),
    CHECK (liquid_reserve_microcredits >= 0),
    CHECK (stressed_liability_microcredits >= 0),
    CHECK (outstanding_quote_exposure_microcredits >= 0),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 128),
    CHECK (jsonb_typeof(metadata) = 'object')
);

CREATE INDEX IF NOT EXISTS risk_state_history_recorded_idx
    ON risk_state_history (recorded_at DESC, id DESC);

CREATE OR REPLACE FUNCTION record_risk_state_history(
    p_reason text,
    p_metadata jsonb DEFAULT '{}'::jsonb
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    state public.risk_state%ROWTYPE;
    history_id bigint;
BEGIN
    IF p_reason IS NULL OR length(btrim(p_reason)) NOT BETWEEN 1 AND 128 THEN
        RAISE EXCEPTION 'risk history reason must be 1..128 characters'
            USING ERRCODE = '22023';
    END IF;
    IF p_metadata IS NULL OR jsonb_typeof(p_metadata) <> 'object' THEN
        RAISE EXCEPTION 'risk history metadata must be a JSON object'
            USING ERRCODE = '22023';
    END IF;

    SELECT * INTO state FROM public.risk_state WHERE singleton FOR UPDATE;
    INSERT INTO public.risk_state_history (
        risk_state_version, valuation_snapshot_id, risk_policy_version_id,
        liquid_reserve_microcredits, stressed_liability_microcredits,
        outstanding_quote_exposure_microcredits, reason, metadata
    ) VALUES (
        state.version, state.valuation_snapshot_id, state.risk_policy_version_id,
        state.liquid_reserve_microcredits, state.stressed_liability_microcredits,
        state.outstanding_quote_exposure_microcredits, btrim(p_reason), p_metadata
    ) RETURNING id INTO history_id;
    RETURN history_id;
END;
$function$;

REVOKE ALL ON TABLE risk_state_history FROM PUBLIC;
REVOKE ALL ON FUNCTION record_risk_state_history(text, jsonb) FROM PUBLIC;

CREATE OR REPLACE FUNCTION read_risk_state_history(
    p_limit integer DEFAULT 100
) RETURNS TABLE (
    id bigint, risk_state_version bigint, valuation_snapshot_id bigint,
    risk_policy_version_id bigint, liquid_reserve_microcredits bigint,
    stressed_liability_microcredits bigint,
    outstanding_quote_exposure_microcredits bigint,
    reason text, metadata jsonb, recorded_at timestamptz
)
LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog STABLE
AS $function$
    SELECT h.id, h.risk_state_version, h.valuation_snapshot_id,
           h.risk_policy_version_id, h.liquid_reserve_microcredits,
           h.stressed_liability_microcredits,
           h.outstanding_quote_exposure_microcredits,
           h.reason, h.metadata, h.recorded_at
      FROM public.risk_state_history AS h
     WHERE p_limit BETWEEN 1 AND 1000
     ORDER BY h.recorded_at DESC, h.id DESC
     LIMIT p_limit;
$function$;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_readonly') THEN
        GRANT EXECUTE ON FUNCTION read_risk_state_history(integer)
            TO contracter_readonly;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime') THEN
        GRANT EXECUTE ON FUNCTION record_risk_state_history(text, jsonb)
            TO contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION read_risk_state_history(integer)
            TO contracter_admin_runtime;
    END IF;
END;
$block$;

CREATE OR REPLACE FUNCTION reject_risk_history_mutation()
RETURNS trigger LANGUAGE plpgsql SET search_path = pg_catalog AS $function$
BEGIN
    RAISE EXCEPTION 'risk_state_history is append-only; write a compensating event'
        USING ERRCODE = '55000';
END;
$function$;

DROP TRIGGER IF EXISTS risk_history_no_update_delete ON risk_state_history;
CREATE TRIGGER risk_history_no_update_delete
BEFORE UPDATE OR DELETE ON risk_state_history FOR EACH STATEMENT
EXECUTE FUNCTION reject_risk_history_mutation();

DROP TRIGGER IF EXISTS risk_history_owner_insert_only ON risk_state_history;
CREATE TRIGGER risk_history_owner_insert_only
BEFORE INSERT ON risk_state_history FOR EACH STATEMENT
EXECUTE FUNCTION require_journal_owner_insert();
