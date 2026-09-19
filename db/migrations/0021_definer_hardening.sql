-- Harden every SECURITY DEFINER function, and stop the owner view
-- handing out internal identifiers.
--
-- Three findings from an independent security review, all confirmed
-- against a live database before this migration was written.

-- 1. Definer ownership (HIGH).
--
-- All 19 SECURITY DEFINER functions are owned by a superuser, because
-- nothing ever reassigned them. A SECURITY DEFINER function runs with its
-- owner's rights, so every one of them currently runs as superuser: any
-- future bug in any one body -- a dynamic-SQL branch, a type confusion, an
-- exploitable RAISE -- executes with full database read and write, plus
-- COPY ... PROGRAM. It also silently defeats the column- and table-level
-- REVOKEs in 0013 and 0019, since a definer can always see everything.
--
-- The fix is a dedicated owner that holds only what these functions need.
-- The role is provisioned outside the migrations, like every other role in
-- this project, because a managed PostgreSQL may not grant CREATEROLE to
-- the migration user. When it is absent this migration says so loudly
-- rather than passing quietly: a hardening step that silently does nothing
-- is worse than one that is known to be pending.
-- The append-only guard has to learn about the new owner first.
--
-- `require_journal_owner_insert` (0005) permits a journal INSERT only when
-- `current_user` equals the *table's* owner. That is what makes the guard
-- unbypassable: a SECURITY DEFINER writer runs as its own owner, so the
-- check cannot be satisfied by a session setting or by a grant.
--
-- Splitting function ownership away from table ownership breaks that
-- equality, and the canonical verification caught it immediately: every
-- ledger write failed with "direct INSERT into ledger_transactions is
-- forbidden".
--
-- The tempting fix -- give the journal tables to contracter_definer too --
-- is the wrong one. A table's owner may `ALTER TABLE ... DISABLE TRIGGER`,
-- so it would hand the definer the ability to switch the append-only
-- guarantee off, which is precisely the power this migration exists to
-- take away. Instead the guard learns one additional permitted writer by
-- name. A role name in the function body is not something a caller can
-- set, so the original property survives intact.
CREATE OR REPLACE FUNCTION require_journal_owner_insert()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, pg_temp
AS $function$
DECLARE
    relation_owner name;
BEGIN
    SELECT pg_get_userbyid(relation.relowner)
      INTO relation_owner
      FROM pg_class AS relation
     WHERE relation.oid = TG_RELID;

    -- A SECURITY DEFINER writer runs as its own owner, which is either the
    -- table owner (the original arrangement) or the dedicated definer role
    -- introduced below. A runtime role is neither, and cannot become
    -- either through a custom session setting.
    IF current_user <> relation_owner AND current_user <> 'contracter_definer' THEN
        RAISE EXCEPTION 'direct INSERT into % is forbidden', TG_TABLE_NAME
            USING ERRCODE = '42501';
    END IF;

    RETURN NULL;
END;
$function$;

-- The owner must be granted what these functions touch BEFORE it owns
-- them. A definer runs with its owner's rights, so reassigning ownership
-- to a role with no table privileges does not harden the functions, it
-- breaks every one of them -- verified directly: after reassignment and
-- before these grants, `find_user_credential_by_login` failed with
-- "permission denied for table users".
--
-- The grant is broad on purpose. What this migration removes is not table
-- access, which these functions genuinely need, but *superuser*: the
-- ability to read and write the filesystem, run COPY ... PROGRAM, bypass
-- row-level security, and act on objects outside this application. The
-- append-only triggers and CHECK constraints still bind the definer
-- exactly as they bind anyone else, because they are enforced by the
-- tables rather than by privileges.
DO $block$
DECLARE
    target record;
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_definer') THEN
        RAISE WARNING 'SKIPPED (not applied): SECURITY DEFINER functions still run as their current owner. Role contracter_definer does not exist, so ownership was not reassigned. Provision it and re-run this migration before treating definer-rights hardening as done.';
        RETURN;
    END IF;

    GRANT USAGE ON SCHEMA public TO contracter_definer;
    GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public
        TO contracter_definer;
    GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO contracter_definer;

    FOR target IN
        SELECT p.oid::regprocedure AS signature
        FROM pg_proc AS p
        JOIN pg_namespace AS n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public' AND p.prosecdef
    LOOP
        EXECUTE format('ALTER FUNCTION %s OWNER TO contracter_definer', target.signature);
    END LOOP;
END;
$block$;

-- 2. search_path completeness (LOW, hardening).
--
-- Every definer function sets `search_path = pg_catalog`. PostgreSQL
-- searches an unlisted `pg_temp` *first*, so the safe form names it
-- explicitly and last. This is hardening rather than a live hole: the
-- review tested the hijack directly (creating `pg_temp.lower(text)` and
-- calling `find_user_credential_by_login`) and it had no effect, because
-- PostgreSQL does not resolve functions or operators from `pg_temp` and
-- every table reference in these bodies is already `public.`-qualified.
-- Stating it is still cheap, and removes the question.
DO $block$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT p.oid::regprocedure AS signature
        FROM pg_proc AS p
        JOIN pg_namespace AS n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public'
          AND p.prosecdef
          AND 'search_path=pg_catalog' = ANY (p.proconfig)
    LOOP
        EXECUTE format(
            'ALTER FUNCTION %s SET search_path = pg_catalog, pg_temp',
            target.signature
        );
    END LOOP;
END;
$block$;

-- 3. The owner view stops exposing internal identifiers (MEDIUM).
--
-- `owned_inventory` (0017) selected `item.id`, the sequential
-- `inventory_items` primary key. Nothing reads it: the projection in
-- `crates/db/src/inventory.rs` selects public ids only. It was one
-- `SELECT *` away from crossing a boundary this project states plainly --
-- public UUIDs leave the database, internal i64 do not.
--
-- `owner_user_id` stays, because it is the predicate the owner listing
-- filters on and there is no way to scope the view without it.
--
-- Dropped and recreated rather than replaced: CREATE OR REPLACE VIEW
-- cannot remove a column, and the grant is restated below in any case.
DROP VIEW IF EXISTS owned_inventory;

CREATE VIEW owned_inventory
WITH (security_barrier = true)
AS
SELECT item.public_id,
       item.created_at,
       item.canonical_float,
       position.owner_user_id,
       EXISTS (
           SELECT 1
           FROM inventory_item_locks AS item_lock
           WHERE item_lock.inventory_item_id = item.id
             AND item_lock.expires_at > clock_timestamp()
       ) AS locked,
       skus.public_id AS sku_public_id,
       catalog_items.public_id AS catalog_item_public_id,
       collections.public_id AS collection_public_id,
       collections.display_name AS collection_display_name,
       catalog_items.stable_name,
       catalog_items.rarity_code,
       wear_bands.code AS wear_band_code,
       catalog_items.is_stattrak,
       catalog_items.is_souvenir
FROM inventory_items AS item
JOIN inventory_positions AS position ON position.inventory_item_id = item.id
JOIN skus ON skus.id = item.sku_id
JOIN catalog_items ON catalog_items.id = skus.catalog_item_id
JOIN collections ON collections.id = catalog_items.collection_id
JOIN wear_bands ON wear_bands.id = skus.wear_band_id
WHERE position.owner_user_id IS NOT NULL
  AND item.retired_at IS NULL;

REVOKE ALL ON owned_inventory FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT SELECT ON owned_inventory TO contracter_runtime;
    END IF;
END;
$block$;
