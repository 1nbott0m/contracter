CREATE TABLE IF NOT EXISTS admin_totp_attempts (
  session_id bigint PRIMARY KEY REFERENCES user_sessions(id) ON DELETE CASCADE,
  failed_attempts integer NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
  locked_until timestamptz
);

CREATE OR REPLACE FUNCTION admin_totp_attempt_allowed(p_session_public_id uuid)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
DECLARE v_allowed boolean;
BEGIN
  SELECT (s.revoked_at IS NULL AND s.expires_at > clock_timestamp()
          AND (a.locked_until IS NULL OR a.locked_until <= clock_timestamp()))
    INTO v_allowed
    FROM user_sessions s
    LEFT JOIN admin_totp_attempts a ON a.session_id=s.id
   WHERE s.public_id=p_session_public_id;
  RETURN COALESCE(v_allowed, false);
END $$;

CREATE OR REPLACE FUNCTION record_admin_totp_failure(p_session_public_id uuid)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
DECLARE v_session_id bigint; v_attempts integer;
BEGIN
  SELECT id INTO v_session_id FROM user_sessions
   WHERE public_id=p_session_public_id AND revoked_at IS NULL AND expires_at > clock_timestamp();
  IF v_session_id IS NULL THEN RETURN false; END IF;
  INSERT INTO admin_totp_attempts(session_id, failed_attempts, locked_until)
  VALUES (v_session_id, 1, NULL)
  ON CONFLICT (session_id) DO UPDATE
    SET failed_attempts = admin_totp_attempts.failed_attempts + 1,
        locked_until = CASE WHEN admin_totp_attempts.failed_attempts + 1 >= 5
                            THEN clock_timestamp() + interval '15 minutes'
                            ELSE admin_totp_attempts.locked_until END
  RETURNING failed_attempts INTO v_attempts;
  RETURN v_attempts < 5;
END $$;

CREATE OR REPLACE FUNCTION clear_admin_totp_failures(p_session_public_id uuid)
RETURNS boolean LANGUAGE sql SECURITY DEFINER SET search_path = public AS $$
  DELETE FROM admin_totp_attempts a USING user_sessions s
   WHERE a.session_id=s.id AND s.public_id=p_session_public_id
   RETURNING true;
$$;

REVOKE ALL ON FUNCTION admin_totp_attempt_allowed(uuid), record_admin_totp_failure(uuid), clear_admin_totp_failures(uuid) FROM PUBLIC;
DO $block$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
    GRANT EXECUTE ON FUNCTION admin_totp_attempt_allowed(uuid), record_admin_totp_failure(uuid), clear_admin_totp_failures(uuid) TO contracter_runtime;
  END IF;
END $block$;
