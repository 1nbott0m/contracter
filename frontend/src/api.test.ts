import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ApiRequestError,
  api,
  inventoryItemToSkin,
  isDevelopmentFallbackEnabled,
} from './api';

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

describe('API failures and endpoint contracts', () => {
  it('preserves the backend status, stable code and request id for callers', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      error: {
        code: 'UNAUTHORIZED',
        message: 'Authentication required',
        request_id: 'fe119d11-f65b-43cb-923b-c06db9394011',
      },
    }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' },
    })));

    const failure = await api.me().catch((error: unknown) => error);

    expect(failure).toBeInstanceOf(ApiRequestError);
    expect(failure).toMatchObject({
      status: 401,
      code: 'UNAUTHORIZED',
      message: 'Authentication required',
      requestId: 'fe119d11-f65b-43cb-923b-c06db9394011',
    });
  });

  it('handles the logout 204 response without attempting to parse JSON', async () => {
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal('fetch', fetchMock);

    await expect(api.logout()).resolves.toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/auth/logout'),
      expect.objectContaining({ method: 'POST', credentials: 'include' }),
    );
  });

  it('finds contract details and verification in the existing history route', async () => {
    const contract = {
      contract_id: 'f7f78654-6db2-49b9-a927-9b75d061523f',
      status: 'completed',
      created_at: '2026-09-24T12:00:00Z',
      ledger_transaction_id: 42,
    };
    const fetchMock = vi.fn(async (_input: RequestInfo | URL) => new Response(JSON.stringify([contract]), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }));
    vi.stubGlobal('fetch', fetchMock);

    await expect(api.contractDetails(contract.contract_id.toUpperCase())).resolves.toEqual(contract);
    await expect(api.verifyContract('missing-contract')).resolves.toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    for (const [url] of fetchMock.mock.calls) {
      expect(new URL(String(url)).pathname).toBe('/api/v1/me/history/contracts');
    }
  });
});

describe('development fallback boundary', () => {
  it('cannot be enabled in production even if the opt-in flag is present', () => {
    expect(isDevelopmentFallbackEnabled({ DEV: false, VITE_ENABLE_DEV_FALLBACK: 'true' })).toBe(false);
  });

  it('requires both development mode and an explicit opt-in', () => {
    expect(isDevelopmentFallbackEnabled({ DEV: true })).toBe(false);
    expect(isDevelopmentFallbackEnabled({ DEV: true, VITE_ENABLE_DEV_FALLBACK: 'false' })).toBe(false);
    expect(isDevelopmentFallbackEnabled({ DEV: true, VITE_ENABLE_DEV_FALLBACK: 'true' })).toBe(true);
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
