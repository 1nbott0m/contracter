import unittest

from import_market_csgo import rows_from_payload
from import_skin_catalog import make_sql


class ImportNormalizationTests(unittest.TestCase):
    def test_history_rows_are_normalized_and_bad_rows_are_ignored(self):
        payload = {"data": {"AK-47 | Test": {"history": [[1_700_000_000, "12.50"], ["bad", 1]]}}}
        rows = rows_from_payload(payload, "sku", "AK-47 | Test")
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["price_rub"], "12.50")
        self.assertEqual(rows[0]["sku_public_id"], "sku")

    def test_malformed_payload_does_not_create_evidence(self):
        self.assertEqual(rows_from_payload({"data": {"item": {"history": "bad"}}}, "sku", "item"), [])
        self.assertEqual(rows_from_payload([], "sku", "item"), [])

    def test_catalog_import_requests_virtual_stock_bootstrap(self):
        statement = make_sql([{
            "id": "skin-id", "name": "Slate", "weapon": "AK-47",
            "rarity": "restricted", "collection": "The Collection",
            "image": "https://example.test/skin.png", "wears": ["minimal_wear"],
            "min_float": 0, "max_float": 1,
        }])
        self.assertIn("SELECT ensure_virtual_warehouse_stock();", statement)


if __name__ == "__main__":
    unittest.main()
