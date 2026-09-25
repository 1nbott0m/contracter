-- A commitment is allocated before client seed or inventory choices arrive.
-- The application encrypts the 32-byte seed before calling this function.
CREATE SEQUENCE IF NOT EXISTS seed_commitment_sequence;

SELECT setval(
    'public.seed_commitment_sequence',
    GREATEST(1, COALESCE((SELECT max(sequence_number) FROM seed_commitments), 0)),
    true
);

CREATE OR REPLACE FUNCTION allocate_seed_for_user(
    p_user_id bigint,
    p_commitment_hash bytea,
    p_encoding_version text,
    p_nonce bytea,
    p_ciphertext bytea
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    commitment_id bigint;
    allocation_public_id uuid;
    allocation_time timestamptz := clock_timestamp();
BEGIN
    IF p_user_id IS NULL OR p_encoding_version = '' OR
       octet_length(p_commitment_hash) <> 32 OR octet_length(p_nonce) <> 24 OR
       octet_length(p_ciphertext) <> 48 THEN
        RAISE EXCEPTION 'invalid seed allocation payload' USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(p_user_id);

    UPDATE public.seed_allocations
       SET released_at = allocation_time
     WHERE user_id = p_user_id AND released_at IS NULL
       AND expires_at <= allocation_time;

    INSERT INTO public.seed_commitments (sequence_number, commitment_hash, encoding_version)
    VALUES (nextval('public.seed_commitment_sequence'), p_commitment_hash, p_encoding_version)
    RETURNING id INTO commitment_id;

    INSERT INTO public.seed_secret_envelopes (commitment_id, nonce, ciphertext)
    VALUES (commitment_id, p_nonce, p_ciphertext);

    INSERT INTO public.seed_allocations (commitment_id, user_id, allocated_at, expires_at)
    VALUES (commitment_id, p_user_id, allocation_time, allocation_time + interval '15 seconds')
    RETURNING public_id INTO allocation_public_id;

    RETURN allocation_public_id;
END;
$function$;

REVOKE ALL ON FUNCTION allocate_seed_for_user(bigint, bytea, text, bytea, bytea) FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION allocate_seed_for_user(bigint, bytea, text, bytea, bytea)
            TO contracter_runtime;
    END IF;
END;
$block$;
