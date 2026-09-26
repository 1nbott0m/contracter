-- Creates the NOLOGIN roles that the privilege assertions in db/tests and the
-- runtime-role Rust tests rely on. Idempotent. Test/CI clusters only: in
-- production, roles are provisioned by the operator, never by this file.
-- Run as a superuser BEFORE db/verify.sh, then export
-- TEST_RUNTIME_ROLE_REQUIRED=1 so a missing role fails instead of skipping.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        CREATE ROLE contracter_runtime NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime') THEN
        CREATE ROLE contracter_admin_runtime NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_readonly') THEN
        CREATE ROLE contracter_readonly NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_definer') THEN
        CREATE ROLE contracter_definer NOLOGIN;
    END IF;
END
$$;
