-- Public registration and a narrow administrator-role projection.
-- Password hashing remains in the application; SQL only stores the PHC hash.
CREATE OR REPLACE FUNCTION register_public_user(
    p_login text,
    p_password_hash text
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    v_public_id uuid;
BEGIN
    IF p_password_hash IS NULL OR p_password_hash = '' THEN
        RAISE EXCEPTION 'password hash is required' USING ERRCODE = '23514';
    END IF;
    INSERT INTO public.users (login, password_hash)
    VALUES (p_login, p_password_hash)
    RETURNING public_id INTO v_public_id;
    RETURN v_public_id;
END;
$function$;

CREATE OR REPLACE FUNCTION is_active_administrator(p_user_public_id uuid)
RETURNS boolean
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT EXISTS (
        SELECT 1
          FROM public.administrators AS administrator
          JOIN public.users AS app_user ON app_user.id = administrator.user_id
         WHERE app_user.public_id = p_user_public_id
           AND app_user.disabled_at IS NULL
           AND administrator.is_active
           AND administrator.deactivated_at IS NULL
    );
$function$;

REVOKE ALL ON FUNCTION register_public_user(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION is_active_administrator(uuid) FROM PUBLIC;
DO $do$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION register_public_user(text, text) TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION is_active_administrator(uuid) TO contracter_runtime;
    END IF;
END;
$do$;
