ALTER TABLE administrators DROP CONSTRAINT IF EXISTS administrators_totp_secret_hash_check;
ALTER TABLE administrators ADD CONSTRAINT administrators_totp_secret_encrypted_check
  CHECK (totp_secret_hash IS NULL OR octet_length(totp_secret_hash) BETWEEN 72 AND 128);

CREATE OR REPLACE FUNCTION administrator_totp_secret(p_user_public_id uuid) RETURNS bytea
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT a.totp_secret_hash FROM administrators a JOIN users u ON u.id=a.user_id
 WHERE u.public_id=p_user_public_id AND a.is_active AND a.deactivated_at IS NULL;
$$;
CREATE OR REPLACE FUNCTION set_administrator_totp_secret(p_user_public_id uuid, p_secret bytea) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
BEGIN
 IF octet_length(p_secret) NOT BETWEEN 72 AND 128 THEN RETURN false; END IF;
 UPDATE administrators a SET totp_secret_hash=p_secret FROM users u
 WHERE u.id=a.user_id AND u.public_id=p_user_public_id AND a.is_active AND a.deactivated_at IS NULL;
 RETURN FOUND;
END $$;
CREATE OR REPLACE FUNCTION mark_session_totp_verified(p_session_public_id uuid) RETURNS boolean
LANGUAGE sql SECURITY DEFINER SET search_path = public AS $$
 UPDATE user_sessions SET totp_verified_at=clock_timestamp()
 WHERE public_id=p_session_public_id AND revoked_at IS NULL AND expires_at>clock_timestamp() RETURNING true;
$$;
CREATE OR REPLACE FUNCTION session_totp_verified(p_session_public_id uuid) RETURNS boolean
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT EXISTS (SELECT 1 FROM user_sessions WHERE public_id=p_session_public_id AND revoked_at IS NULL AND expires_at>clock_timestamp() AND totp_verified_at IS NOT NULL);
$$;
REVOKE ALL ON FUNCTION administrator_totp_secret(uuid), set_administrator_totp_secret(uuid,bytea), mark_session_totp_verified(uuid), session_totp_verified(uuid) FROM PUBLIC;
DO $block$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
    GRANT EXECUTE ON FUNCTION administrator_totp_secret(uuid), set_administrator_totp_secret(uuid,bytea), mark_session_totp_verified(uuid), session_totp_verified(uuid) TO contracter_runtime;
  END IF;
END $block$;
