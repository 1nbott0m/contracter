# Canonical CS2 skin catalog

The browser must not import a third-party dataset. Run the importer as a
server-side sync job, review the generated SQL, and apply it to the
CONTRACTER database.

```sh
python3 scripts/import_skin_catalog.py \
  https://raw.githubusercontent.com/ByMykel/CSGO-API/main/public/api/en/skins.json \
  --database-url "$DATABASE_URL" \
  --execute \
  --mirror-dir ./var/skin-cdn \
  --image-base-url https://cdn.example.com/contracter
```

The job normalizes one canonical skin finish into `catalog_items`, stores its
`canonical_skin_id`, weapon, skin name, collection, wear list and canonical
image URL, then creates one `skus` row for every supported wear band. Virtual
inventory instances continue to reference the SKU; they do not copy artwork.

`--mirror-dir` is optional for development. In production it should point to
the object-storage sync destination, and `--image-base-url` should be the
immutable CDN origin. If mirroring fails, metadata import continues but the
job reports the failed image so the deployment can retry it.

The current frontend uses the same canonical image URL returned by
`GET /api/v1/catalog/skus`. Its image component reserves card dimensions,
lazy-loads the artwork, fades it in, and shows a neutral silhouette when the
configured source is unavailable.
