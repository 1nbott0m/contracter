import { describe, expect, it } from 'vitest';
import { eligibleContractItems } from './contractEligibility';

const item = (id: string, skuId: string, collectionId = 'collection-a') => ({
  id, publicId: id, skuId, collectionId, collectionDisplayName: collectionId,
  weapon: 'AK-47', skin: id, wear: 'factory new', price: 100,
  color: '#58d6e7', rarity: 'restricted', image: '', canonicalImageUrl: null,
  canonicalFloat: '0.1', isStatTrak: false, isSouvenir: false, locked: false,
  acquiredAt: '2026-01-01T00:00:00Z',
});

const sku = (skuId: string, itemId: string, rank: number, collectionId = 'collection-a') => ({
  sku_id: skuId, item_id: itemId, collection_id: collectionId, collection_slug: collectionId,
  collection_display_name: collectionId, stable_name: itemId, rarity: rank === 2 ? 'restricted' : 'classified',
  rarity_rank: rank, wear_band: 'factory_new', min_float: '0', max_float: '1',
  is_stattrak: false, is_souvenir: false, canonical_skin_id: null, weapon: 'AK-47',
  skin_name: itemId, canonical_image_url: 'https://cdn.test/item.png', available_wears: ['factory_new'],
});

describe('eligibleContractItems', () => {
  it('hides items without an available next-tier outcome', () => {
    const items = [item('eligible', 'sku-1'), item('missing-output', 'sku-2', 'collection-b')];
    const catalog = [sku('sku-1', 'eligible', 2), sku('out-1', 'output', 3)];
    const valuations = [
      { sku_id: 'sku-1', price_microcredits: 100_000_000, currency_code: 'CC' as const, updated_at: '', available: true },
      { sku_id: 'out-1', price_microcredits: 200_000_000, currency_code: 'CC' as const, updated_at: '', available: true },
    ];
    expect(eligibleContractItems(items, catalog, valuations).map((entry) => entry.item.id)).toEqual(['eligible']);
  });

  it('keeps selected rarity and collection compatible and rejects the 15000 CC input cap', () => {
    const items = [item('first', 'sku-1'), item('same', 'sku-2'), item('other-rarity', 'sku-3')];
    const catalog = [sku('sku-1', 'first', 2), sku('sku-2', 'same', 2), sku('sku-3', 'other-rarity', 3), sku('out', 'output', 3)];
    const valuations = [
      ...items.map((entry) => ({ sku_id: entry.skuId!, price_microcredits: entry.id === 'first' ? 10_000_000_000 : 5_000_000_000, currency_code: 'CC' as const, updated_at: '', available: true })),
      { sku_id: 'out', price_microcredits: 1_000_000, currency_code: 'CC' as const, updated_at: '', available: true },
    ];
    const result = eligibleContractItems(items, catalog, valuations, [items[0]]);
    expect(result.map((entry) => entry.item.id)).toEqual(['first']);
  });
});
