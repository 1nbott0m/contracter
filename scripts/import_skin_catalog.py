#!/usr/bin/env python3
"""Import a normalized CS2 skin dataset into CONTRACTER.

The browser never talks to the external dataset. This job normalizes one
canonical row per skin finish and creates one SKU per supported wear band.
It can emit SQL for review or execute it through psql. Images are optionally
mirrored into a CONTRACTER-controlled directory and the stored URL is then
derived from IMAGE_BASE_URL.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

RARITY = {
    "consumer grade": "consumer", "industrial grade": "industrial",
    "mil-spec grade": "mil-spec", "restricted": "restricted",
    "classified": "classified", "covert": "covert",
    "contraband": "contraband",
}
WEAR = {"factory new": "factory_new", "minimal wear": "minimal_wear",
        "field-tested": "field_tested", "well-worn": "well_worn",
        "battle-scarred": "battle_scarred"}

def sql(value: object) -> str:
    if value is None:
        return "NULL"
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    return "'" + str(value).replace("'", "''") + "'"

def slug(value: str) -> str:
    value = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return value or "unknown-collection"

def rows(payload: object) -> list[dict]:
    if isinstance(payload, dict):
        payload = payload.get("skins", payload.get("items", []))
    if not isinstance(payload, list):
        raise ValueError("dataset must be an array or contain skins/items array")
    return [row for row in payload if isinstance(row, dict)]

def normalize(row: dict, image_base: str | None, mirror_dir: Path | None) -> dict | None:
    canonical_id = str(row.get("id") or row.get("canonical_skin_id") or "").strip()
    name = str(row.get("name") or row.get("skin_name") or "").strip()
    weapon = row.get("weapon")
    weapon = weapon.get("name") if isinstance(weapon, dict) else weapon
    weapon = str(weapon or "").strip()
    if "|" in name:
        parsed_weapon, parsed_skin = (part.strip() for part in name.split("|", 1))
        weapon = weapon or parsed_weapon
        name = parsed_skin
    rarity = row.get("rarity")
    rarity = rarity.get("name") if isinstance(rarity, dict) else rarity
    rarity_code = RARITY.get(str(rarity or "").lower().strip())
    collection = row.get("collection") or row.get("collections")
    if isinstance(collection, list):
        collection = collection[0] if collection else None
    collection = collection.get("name") if isinstance(collection, dict) else collection
    image = str(row.get("image") or row.get("canonical_image_url") or "").strip()
    if not canonical_id or not name or not weapon or not rarity_code or not collection or not image:
        return None
    wears = row.get("wears") or row.get("available_wears") or []
    wear_codes: list[str] = []
    for wear in wears:
        wear_name = wear.get("name") if isinstance(wear, dict) else wear
        code = WEAR.get(str(wear_name or "").lower().strip())
        if code and code not in wear_codes:
            wear_codes.append(code)
    if not wear_codes:
        return None
    if mirror_dir:
        mirror_dir.mkdir(parents=True, exist_ok=True)
        target = mirror_dir / f"{canonical_id}.webp"
        if not target.exists():
            try:
                urllib.request.urlretrieve(image, target)
            except Exception as exc:  # keep importing metadata; report later
                print(f"warning: image mirror failed for {canonical_id}: {exc}", file=sys.stderr)
        if target.exists() and image_base:
            image = f"{image_base.rstrip('/')}/skins/{canonical_id}.webp"
    return {"id": canonical_id, "name": name, "weapon": weapon,
            "rarity": rarity_code, "collection": str(collection),
            "image": image, "wears": wear_codes,
            "min_float": row.get("min_float", 0), "max_float": row.get("max_float", 1)}

def make_sql(items: list[dict]) -> str:
    out = ["BEGIN;", "SET LOCAL lock_timeout = '5s';"]
    for item in items:
        collection_slug = slug(item["collection"])
        wears_json = json.dumps(item["wears"], separators=(",", ":"))
        out.append(
            "INSERT INTO collections (slug, display_name) VALUES "
            f"({sql(collection_slug)}, {sql(item['collection'])}) "
            "ON CONFLICT (slug) DO UPDATE SET display_name = EXCLUDED.display_name;"
        )
        out.append(
            "INSERT INTO catalog_items (collection_id, rarity_code, stable_name, min_float, max_float, "
            "canonical_skin_id, weapon_name, skin_name, canonical_image_url, available_wears) "
            f"SELECT id, {sql(item['rarity'])}, {sql(item['name'])}, {sql(item['min_float'])}, {sql(item['max_float'])}, "
            f"{sql(item['id'])}, {sql(item['weapon'])}, {sql(item['name'])}, {sql(item['image'])}, {sql(wears_json)}::jsonb "
            f"FROM collections WHERE slug = {sql(collection_slug)} "
            "ON CONFLICT (canonical_skin_id) WHERE canonical_skin_id IS NOT NULL DO UPDATE SET rarity_code=EXCLUDED.rarity_code, "
            "min_float=EXCLUDED.min_float, max_float=EXCLUDED.max_float, canonical_skin_id=EXCLUDED.canonical_skin_id, "
            "weapon_name=EXCLUDED.weapon_name, skin_name=EXCLUDED.skin_name, canonical_image_url=EXCLUDED.canonical_image_url, "
            "available_wears=EXCLUDED.available_wears;"
        )
        for wear in item["wears"]:
            out.append(
                "INSERT INTO skus (catalog_item_id, wear_band_id) "
                f"SELECT ci.id, wb.id FROM catalog_items ci JOIN collections c ON c.id=ci.collection_id "
                f"JOIN wear_bands wb ON wb.code={sql(wear)} WHERE ci.canonical_skin_id={sql(item['id'])} "
                "ON CONFLICT (catalog_item_id, wear_band_id) DO UPDATE SET enabled=true;"
            )
    out.append("COMMIT;")
    return "\n".join(out) + "\n"

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("dataset", help="JSON file or URL")
    parser.add_argument("--database-url", default=os.getenv("DATABASE_URL"))
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--mirror-dir", type=Path)
    parser.add_argument("--image-base-url", default=os.getenv("CONTRACTER_IMAGE_BASE_URL"))
    args = parser.parse_args()
    raw = urllib.request.urlopen(args.dataset).read() if args.dataset.startswith("http") else Path(args.dataset).read_bytes()
    items = [x for row in rows(json.loads(raw)) if (x := normalize(row, args.image_base_url, args.mirror_dir))]
    if not items:
        raise SystemExit("dataset produced no valid canonical skins")
    text = make_sql(items)
    if args.execute:
        if not args.database_url:
            raise SystemExit("--database-url or DATABASE_URL is required with --execute")
        subprocess.run(["psql", args.database_url, "-v", "ON_ERROR_STOP=1"], input=text, text=True, check=True)
    else:
        sys.stdout.write(text)
    print(f"normalized {len(items)} canonical skins", file=sys.stderr)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
