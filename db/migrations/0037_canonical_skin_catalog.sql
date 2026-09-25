-- Canonical skin metadata is owned by CONTRACTER after import.  Virtual
-- item/SKU rows refer to this definition and never duplicate its artwork.
ALTER TABLE catalog_items
    ADD COLUMN IF NOT EXISTS canonical_skin_id text,
    ADD COLUMN IF NOT EXISTS weapon_name text,
    ADD COLUMN IF NOT EXISTS skin_name text,
    ADD COLUMN IF NOT EXISTS canonical_image_url text,
    ADD COLUMN IF NOT EXISTS available_wears jsonb NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE catalog_items
    DROP CONSTRAINT IF EXISTS catalog_items_collection_id_stable_name_key;

CREATE UNIQUE INDEX IF NOT EXISTS catalog_items_canonical_skin_id_uq
    ON catalog_items (canonical_skin_id)
    WHERE canonical_skin_id IS NOT NULL;

ALTER TABLE catalog_items
    DROP CONSTRAINT IF EXISTS catalog_items_canonical_image_url_check;
ALTER TABLE catalog_items
    ADD CONSTRAINT catalog_items_canonical_image_url_check
    CHECK (canonical_image_url IS NULL OR canonical_image_url ~ '^https?://');

COMMENT ON COLUMN catalog_items.canonical_skin_id IS
    'Stable id from the imported external skin dataset; one image per finish';
COMMENT ON COLUMN catalog_items.canonical_image_url IS
    'Immutable canonical artwork URL mirrored to CONTRACTER storage/CDN in production';
