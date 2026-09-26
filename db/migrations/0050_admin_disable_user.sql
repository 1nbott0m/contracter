CREATE OR REPLACE FUNCTION admin_disable_user(p_admin_public_id uuid, p_target_public_id uuid)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
DECLARE v_target_id bigint;
BEGIN
  IF p_admin_public_id = p_target_public_id THEN RETURN false; END IF;
  IF NOT EXISTS (
    SELECT 1 FROM administrators a JOIN users u ON u.id=a.user_id
     WHERE u.public_id=p_admin_public_id AND a.is_active AND a.deactivated_at IS NULL
  ) THEN
    RAISE EXCEPTION 'administrator is not active' USING ERRCODE='42501';
  END IF;
  SELECT id INTO v_target_id FROM users WHERE public_id=p_target_public_id AND disabled_at IS NULL;
  IF v_target_id IS NULL THEN RETURN false; END IF;
  UPDATE users SET disabled_at=clock_timestamp() WHERE id=v_target_id;
  UPDATE user_sessions SET revoked_at=clock_timestamp() WHERE user_id=v_target_id AND revoked_at IS NULL;
  PERFORM record_admin_audit(p_admin_public_id, 'admin.user.disable', p_target_public_id, '{}'::jsonb);
  RETURN true;
END $$;

REVOKE ALL ON FUNCTION admin_disable_user(uuid,uuid) FROM PUBLIC;
DO $block$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
    GRANT EXECUTE ON FUNCTION admin_disable_user(uuid,uuid) TO contracter_runtime;
  END IF;
END $block$;
