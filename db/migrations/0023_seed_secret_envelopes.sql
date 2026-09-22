-- Unrevealed server seeds are encrypted by the application before this row is
-- written.  The database never stores plaintext seed material.
CREATE TABLE IF NOT EXISTS seed_secret_envelopes (
    commitment_id bigint PRIMARY KEY REFERENCES seed_commitments(id),
    nonce bytea NOT NULL,
    ciphertext bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (octet_length(nonce) = 24),
    CHECK (octet_length(ciphertext) > 32)
);

REVOKE ALL ON seed_secret_envelopes FROM PUBLIC;

DROP TRIGGER IF EXISTS seed_secret_envelopes_append_only ON seed_secret_envelopes;
CREATE TRIGGER seed_secret_envelopes_append_only
BEFORE UPDATE OR DELETE ON seed_secret_envelopes
FOR EACH STATEMENT EXECUTE FUNCTION public.reject_append_only_mutation();

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE ALL ON seed_secret_envelopes FROM contracter_runtime;
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_readonly') THEN
        REVOKE ALL ON seed_secret_envelopes FROM contracter_readonly;
    END IF;
END;
$block$;
