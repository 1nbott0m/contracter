#!/usr/bin/env python3
"""Import completed Market.CSGO sales into Contracter.

This is deliberately a one-shot server-side job, not an HTTP route. It needs
curl-compatible network access and psql, plus MARKET_CSGO_API_KEY and a DB URL
with EXECUTE on the 0043 importer functions. It never prints the API key.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import urllib.parse
import urllib.request
import urllib.error
from datetime import datetime, timezone
from decimal import Decimal


def required(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        raise SystemExit(f"missing required environment variable: {name}")
    return value


def psql(database_url: str, sql: str, *defines: tuple[str, str]) -> str:
    command = ["psql", "-X", "-A", "-t", "-F", "\t", "-v", "ON_ERROR_STOP=1", database_url]
    for key, value in defines:
        command.extend(["--set", f"{key}={value}"])
    result = subprocess.run(command, input=sql, text=True, capture_output=True, check=False)
    if result.returncode:
        raise SystemExit(result.stderr.strip() or "psql failed")
    return result.stdout.strip()


def api_json(key: str, hash_name: str) -> object:
    query = urllib.parse.urlencode(
        [("key", key), ("list_hash_name[]", hash_name), ("history", "1")]
    )
    request = urllib.request.Request(
        f"https://market.csgo.com/api/v2/get-list-items-info?{query}",
        headers={"Accept": "application/json", "User-Agent": "contracter-market-import/1"},
    )
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as error:
        # Market.CSGO may transiently fail one item while the rest of the
        # catalog remains available. Do not discard a whole import for that.
        if error.code >= 500:
            print(f"skipping temporarily unavailable item {hash_name!r}: HTTP {error.code}", file=sys.stderr)
            return []
        raise SystemExit(f"Market.CSGO request failed with HTTP {error.code}") from error
    except urllib.error.URLError as error:
        print(f"skipping unavailable item {hash_name!r}: {error.reason}", file=sys.stderr)
        return []
    except TimeoutError:
        print(f"skipping timed-out item {hash_name!r}", file=sys.stderr)
        return []
    if isinstance(payload, dict) and payload.get("success") is False:
        raise SystemExit(f"Market.CSGO rejected request: {payload.get('error', 'unknown error')}")
    return payload


def rows_from_payload(payload: object, sku_public_id: str, hash_name: str) -> list[dict[str, str]]:
    """Normalize Market.CSGO's data[name].history [[unix_time, rub], ...]."""
    if isinstance(payload, dict):
        candidates = (
            payload.get("history")
            or payload.get("data")
            or payload.get("items")
            or payload.get("result")
            or []
        )
    else:
        candidates = payload
    if isinstance(candidates, dict):
        details = candidates.get(hash_name)
        if not isinstance(details, dict):
            return []
        history = details.get("history")
        if not isinstance(history, list):
            return []
        rows: list[dict[str, str]] = []
        for index, point in enumerate(history):
            if not isinstance(point, list) or len(point) < 2:
                continue
            timestamp, price = point[0], point[1]
            try:
                price_rub = Decimal(str(price))
                source_timestamp = datetime.fromtimestamp(float(timestamp), tz=timezone.utc).isoformat()
            except (ArithmeticError, ValueError, TypeError, OverflowError):
                continue
            if price_rub <= 0:
                continue
            rows.append({
                "sku_public_id": sku_public_id,
                "external_event_key": f"market-csgo:{hash_name}:{timestamp}:{index}",
                "source_timestamp": source_timestamp,
                "price_rub": format(price_rub, "f"),
            })
        return rows
    if not isinstance(candidates, list):
        return []
    rows: list[dict[str, str]] = []
    for item in candidates:
        if not isinstance(item, dict):
            continue
        price = item.get("price") or item.get("price_rub") or item.get("amount")
        event_key = item.get("id") or item.get("sale_id") or item.get("event_key")
        timestamp = item.get("date") or item.get("created_at") or item.get("timestamp")
        if price is None or event_key is None or timestamp is None:
            continue
        try:
            price_rub = Decimal(str(price))
            if price_rub <= 0:
                continue
            if isinstance(timestamp, (int, float)) or str(timestamp).isdigit():
                source_timestamp = datetime.fromtimestamp(float(timestamp), tz=timezone.utc).isoformat()
            else:
                source_timestamp = str(timestamp)
        except (ArithmeticError, ValueError, TypeError, OverflowError):
            continue
        rows.append({
            "sku_public_id": sku_public_id,
            "external_event_key": f"market-csgo:{event_key}:{hash_name}",
            "source_timestamp": source_timestamp,
            "price_rub": format(price_rub, "f"),
        })
    return rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--publish", action="store_true", help="publish a snapshot after ingest")
    parser.add_argument("--window-days", type=int, choices=(7, 30), default=30)
    parser.add_argument(
        "--max-source-age-hours",
        type=int,
        default=int(os.environ.get("MARKET_MAX_SOURCE_AGE_HOURS", "72")),
        help="fail closed when the source has no completed sale within this age",
    )
    args = parser.parse_args()
    if args.max_source_age_hours <= 0:
        raise SystemExit("--max-source-age-hours must be positive")

    key = required("MARKET_CSGO_API_KEY")
    database_url = os.environ.get("MARKET_IMPORT_DATABASE_URL") or required("DATABASE_URL")
    rate = os.environ.get("MARKET_RUB_TO_CC_RATE", "1")
    catalog = psql(
        database_url,
        """SELECT skus.public_id,
                         catalog_items.weapon_name || ' | ' || catalog_items.skin_name ||
                         ' (' || initcap(replace(wear_bands.code, '_', ' ')) || ')'
                  FROM skus
                  JOIN catalog_items ON catalog_items.id = skus.catalog_item_id
                  JOIN wear_bands ON wear_bands.id = skus.wear_band_id
                 WHERE skus.enabled AND catalog_items.enabled
                 ORDER BY skus.public_id;""",
    )
    all_rows: list[dict[str, str]] = []
    for line in catalog.splitlines():
        sku, hash_name = line.split("\t", 1)
        all_rows.extend(rows_from_payload(api_json(key, hash_name), sku, hash_name))
    if not all_rows:
        raise SystemExit("Market.CSGO returned no usable completed-sale rows; nothing was written")
    parsed_source_times = []
    for row in all_rows:
        parsed = datetime.fromisoformat(row["source_timestamp"].replace("Z", "+00:00"))
        parsed_source_times.append(
            parsed.replace(tzinfo=timezone.utc) if parsed.tzinfo is None else parsed
        )
    newest_source_at = max(parsed_source_times)
    age_seconds = (datetime.now(timezone.utc) - newest_source_at).total_seconds()
    if age_seconds > args.max_source_age_hours * 3600:
        raise SystemExit(
            "Market.CSGO evidence is stale; refusing to ingest or publish a new valuation snapshot"
        )
    inserted_total = 0
    # Keep each psql variable comfortably below shell/argument-size limits.
    for offset in range(0, len(all_rows), 100):
        payload = json.dumps(all_rows[offset : offset + 100], separators=(",", ":"))
        inserted = psql(
            database_url,
            "SELECT ingest_market_sale_evidence('market_csgo', :'rows'::jsonb, :'rate'::numeric);",
            ("rows", payload),
            ("rate", rate),
        )
        inserted_total += int(inserted or "0")
    print(f"inserted sale evidence rows: {inserted_total}")
    if args.publish:
        snapshot = psql(
            database_url,
            "SELECT build_market_valuation_snapshot('market_csgo', 'market-csgo-trimmed-mean-v1', :'days'::smallint);",
            ("days", str(args.window_days)),
        )
        print(f"published valuation snapshot: {snapshot}")
    else:
        print("dry ingest only; rerun with --publish after reviewing evidence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
