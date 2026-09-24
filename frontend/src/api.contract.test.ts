import type { ApiInventoryItem } from './api';

// This mirrors crates/api/src/routes/inventory.rs. Keeping it type-checked
// makes a wire-contract drift fail the frontend build instead of silently
// rendering malformed inventory cards.
const inventoryWireExample = {
  item_id: '92c7c6a6-1d7b-47d2-9ea6-39e9d125676e',
  sku_id: '98c7c6a6-1d7b-47d2-9ea6-39e9d125676e',
  catalog_item_id: '91c7c6a6-1d7b-47d2-9ea6-39e9d125676e',
  collection_id: '90c7c6a6-1d7b-47d2-9ea6-39e9d125676e',
  collection_display_name: 'The Collection',
  stable_name: 'AK-47 | Slate',
  rarity: 'restricted',
  wear_band: 'minimal_wear',
  canonical_float: '0.12345678',
  is_stattrak: false,
  is_souvenir: false,
  locked: false,
  acquired_at: '2026-09-24T12:00:00Z',
} satisfies ApiInventoryItem;

void inventoryWireExample;
