BEGIN;
SET LOCAL lock_timeout = '5s';
INSERT INTO rarities (code, rank, is_covert) VALUES
    ('consumer', 0, false), ('industrial', 1, false), ('mil-spec', 2, false),
    ('restricted', 3, false), ('classified', 4, false), ('covert', 5, true)
ON CONFLICT (code) DO UPDATE SET rank = EXCLUDED.rank, is_covert = EXCLUDED.is_covert;
INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) VALUES
    ('factory_new', 0.00000000, 0.07000000, false), ('minimal_wear', 0.07000000, 0.15000000, false),
    ('field_tested', 0.15000000, 0.38000000, false), ('well_worn', 0.38000000, 0.45000000, false),
    ('battle_scarred', 0.45000000, 1.00000000, true)
ON CONFLICT (code) DO UPDATE SET lower_bound = EXCLUDED.lower_bound, upper_bound = EXCLUDED.upper_bound, includes_upper_bound = EXCLUDED.includes_upper_bound;
INSERT INTO skus (catalog_item_id, wear_band_id)
SELECT ci.id, wb.id FROM catalog_items ci CROSS JOIN wear_bands wb
WHERE ci.canonical_skin_id IS NOT NULL
ON CONFLICT (catalog_item_id, wear_band_id) DO UPDATE SET enabled = true;
COMMIT;
