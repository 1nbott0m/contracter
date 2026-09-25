ALTER TABLE users ADD COLUMN IF NOT EXISTS steam_id text;
CREATE UNIQUE INDEX IF NOT EXISTS users_steam_id_key ON users (steam_id) WHERE steam_id IS NOT NULL;

CREATE OR REPLACE FUNCTION find_user_by_steam_id(p_steam_id text)
RETURNS TABLE(user_id bigint, user_public_id uuid)
LANGUAGE sql SECURITY DEFINER STABLE SET search_path = public AS $$
 SELECT id, public_id FROM users WHERE steam_id=p_steam_id AND disabled_at IS NULL;
$$;
CREATE OR REPLACE FUNCTION register_steam_user(p_login text, p_password_hash text, p_steam_id text)
RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER SET search_path = public AS $$
DECLARE v_public_id uuid;
BEGIN
 INSERT INTO users(login,password_hash,steam_id) VALUES(p_login,p_password_hash,p_steam_id) RETURNING public_id INTO v_public_id;
 RETURN v_public_id;
END $$;
REVOKE ALL ON FUNCTION find_user_by_steam_id(text), register_steam_user(text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION find_user_by_steam_id(text), register_steam_user(text,text,text) TO contracter_runtime;
