CREATE TABLE IF NOT EXISTS admin_audit_events (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
  administrator_id bigint NOT NULL REFERENCES administrators(id),
  action_code text NOT NULL CHECK (length(action_code) BETWEEN 1 AND 96),
  target_public_id uuid,
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(metadata) = 'object'),
  created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX IF NOT EXISTS admin_audit_events_created_idx ON admin_audit_events (created_at DESC);
CREATE OR REPLACE FUNCTION record_admin_audit(p_admin_public_id uuid, p_action_code text, p_target_public_id uuid, p_metadata jsonb)
RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
DECLARE v_event uuid;
BEGIN
 INSERT INTO admin_audit_events(administrator_id, action_code, target_public_id, metadata)
 SELECT a.id, p_action_code, p_target_public_id, COALESCE(p_metadata, '{}'::jsonb)
 FROM administrators a JOIN users u ON u.id=a.user_id
 WHERE u.public_id=p_admin_public_id AND a.is_active AND a.deactivated_at IS NULL
 RETURNING public_id INTO v_event;
 IF v_event IS NULL THEN RAISE EXCEPTION 'administrator is not active' USING ERRCODE='42501'; END IF;
 RETURN v_event;
END $$;
CREATE OR REPLACE FUNCTION list_admin_audit_events()
RETURNS TABLE(public_id uuid, administrator_public_id uuid, action_code text, target_public_id uuid, metadata jsonb, created_at timestamptz)
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT e.public_id, a.public_id, e.action_code, e.target_public_id, e.metadata, e.created_at
 FROM admin_audit_events e JOIN administrators a ON a.id=e.administrator_id ORDER BY e.created_at DESC LIMIT 200;
$$;
REVOKE ALL ON FUNCTION record_admin_audit(uuid,text,uuid,jsonb), list_admin_audit_events() FROM PUBLIC;
DO $block$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
    GRANT EXECUTE ON FUNCTION record_admin_audit(uuid,text,uuid,jsonb), list_admin_audit_events() TO contracter_runtime;
  END IF;
END $block$;
CREATE OR REPLACE FUNCTION prevent_admin_audit_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'admin audit events are append-only'; END $$;
DROP TRIGGER IF EXISTS admin_audit_events_immutable ON admin_audit_events;
CREATE TRIGGER admin_audit_events_immutable BEFORE UPDATE OR DELETE ON admin_audit_events FOR EACH ROW EXECUTE FUNCTION prevent_admin_audit_mutation();
