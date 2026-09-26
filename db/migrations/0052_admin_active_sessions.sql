CREATE OR REPLACE FUNCTION list_admin_active_sessions(p_admin_public_id uuid)
RETURNS TABLE(session_id uuid, user_id uuid, login text, created_at timestamptz, expires_at timestamptz, totp_verified boolean)
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT s.public_id, u.public_id, u.login, s.created_at, s.expires_at, s.totp_verified_at IS NOT NULL
   FROM user_sessions s JOIN users u ON u.id=s.user_id
  WHERE s.revoked_at IS NULL AND s.expires_at > clock_timestamp()
    AND EXISTS (SELECT 1 FROM administrators ad JOIN users au ON au.id=ad.user_id
                 WHERE au.public_id=p_admin_public_id AND ad.is_active AND ad.deactivated_at IS NULL)
  ORDER BY s.created_at DESC LIMIT 500;
$$;

REVOKE ALL ON FUNCTION list_admin_active_sessions(uuid) FROM PUBLIC;
DO $block$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
    GRANT EXECUTE ON FUNCTION list_admin_active_sessions(uuid) TO contracter_runtime;
  END IF;
END $block$;
