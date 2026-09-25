-- Narrow owner-bound read path for the application decryptor.  The runtime
-- role never receives table privileges and can only read an active allocation
-- through this function.
CREATE OR REPLACE FUNCTION public.read_seed_envelope_for_user(
    p_user_id bigint,
    p_allocation_public_id uuid
)
RETURNS TABLE (
    allocation_public_id uuid,
    commitment_hash bytea,
    nonce bytea,
    ciphertext bytea
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $function$
    SELECT allocation.public_id,
           commitment.commitment_hash,
           envelope.nonce,
           envelope.ciphertext
      FROM public.seed_allocations AS allocation
      JOIN public.seed_commitments AS commitment
        ON commitment.id = allocation.commitment_id
      JOIN public.seed_secret_envelopes AS envelope
        ON envelope.commitment_id = commitment.id
     WHERE allocation.user_id = p_user_id
       AND allocation.public_id = p_allocation_public_id
       AND allocation.released_at IS NULL
       AND allocation.expires_at > clock_timestamp();
$function$;

REVOKE ALL ON FUNCTION public.read_seed_envelope_for_user(bigint, uuid) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION public.read_seed_envelope_for_user(bigint, uuid)
            TO contracter_runtime;
    END IF;
END;
$block$;
