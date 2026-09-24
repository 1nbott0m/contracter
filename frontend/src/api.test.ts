import { afterEach, describe, expect, it, vi } from 'vitest';
import { api, inventoryItemToSkin } from './api';

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

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('paginated API lists', () => {
  it.each([
    ['inventory', '/api/v1/me/inventory'],
    ['catalogSkus', '/api/v1/catalog/skus'],
    ['marketValuations', '/api/v1/market/valuations'],
  ] as const)('loads every %s page by following the opaque cursor', async (method, pathname) => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ items: [{ page: 1 }], next_cursor: 'opaque cursor/+=' }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ items: [{ page: 2 }] }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }));
    vi.stubGlobal('fetch', fetchMock);

    const result = await api[method]();

    expect(result.items).toEqual([{ page: 1 }, { page: 2 }]);
    expect(result.next_cursor).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledTimes(2);

    const firstUrl = new URL(String(fetchMock.mock.calls[0]?.[0]));
    expect(firstUrl.pathname).toBe(pathname);
    expect(firstUrl.searchParams.get('limit')).toBe('200');
    expect(firstUrl.searchParams.has('cursor')).toBe(false);

    const secondUrl = new URL(String(fetchMock.mock.calls[1]?.[0]));
    expect(secondUrl.pathname).toBe(pathname);
    expect(secondUrl.searchParams.get('limit')).toBe('200');
    expect(secondUrl.searchParams.get('cursor')).toBe('opaque cursor/+=');
  });

  it('stops with an error if a server repeats a cursor instead of looping forever', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      items: [],
      next_cursor: 'same-cursor',
    }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    })));

    await expect(api.inventory()).rejects.toThrow('API_PAGINATION_CURSOR_REPEATED');
  });
});

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

  it('marks a missing market valuation as unavailable instead of inventing a zero price', () => {
    const result = inventoryItemToSkin(inventoryItem, catalogSku);

    expect(result.price).toBeNull();
  });
});
