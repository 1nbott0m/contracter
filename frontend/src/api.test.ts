import { describe, expect, it } from 'vitest';
import { inventoryItemToSkin } from './api';

const inventoryItem = {
  item_id: 'inventory-item',
  sku_id: 'sku',
  catalog_item_id: 'catalog-item',
  collection_id: 'collection',
  collection_display_name: 'The Collection',
  stable_name: 'legacy stable name',
  rarity: 'restricted',
  wear_band: 'minimal_wear',
  canonical_float: '0.12345678',
  is_stattrak: false,
  is_souvenir: false,
  locked: false,
  acquired_at: '2026-09-24T12:00:00Z',
};

const catalogSku = {
  sku_id: 'sku',
  item_id: 'catalog-item',
  collection_id: 'collection',
  collection_slug: 'the-collection',
  collection_display_name: 'The Collection',
  stable_name: 'AK-47 | Slate',
  rarity: 'restricted',
  rarity_rank: 3,
  wear_band: 'minimal_wear',
  min_float: '0.07',
  max_float: '0.15',
  is_stattrak: false,
  is_souvenir: false,
  canonical_skin_id: 'ak47-slate',
  weapon: 'AK-47',
  skin_name: 'Slate',
  canonical_image_url: 'https://cdn.example.test/ak47-slate.png',
  available_wears: ['minimal_wear'],
};

const valuation = {
  sku_id: 'sku',
  price_microcredits: 1_234_567,
  currency_code: 'CC' as const,
  updated_at: '2026-09-24T12:00:00Z',
  available: true,
};

describe('inventoryItemToSkin', () => {
  it('joins canonical catalog metadata instead of presenting a collection as a weapon', () => {
    const result = inventoryItemToSkin(inventoryItem, catalogSku, valuation);

    expect(result.weapon).toBe('AK-47');
    expect(result.skin).toBe('Slate');
    expect(result.canonicalImageUrl).toBe('https://cdn.example.test/ak47-slate.png');
  });

  it('uses the public market valuation as the owned item price', () => {
    const result = inventoryItemToSkin(inventoryItem, catalogSku, valuation);

    expect(result.price).toBe(1.234567);
  });
});
