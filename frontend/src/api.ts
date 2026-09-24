import type { InventoryItem as SkinInventoryItem } from './types';

export type ApiPage<T> = { items: T[]; next_cursor?: string | null };
/** Exact JSON shape returned by `GET /api/v1/me/inventory`. UUIDs and floats
 * remain strings: parsing them as numbers would lose identity/precision. */
export type ApiInventoryItem = {
  item_id: string;
  sku_id: string;
  catalog_item_id: string;
  collection_id: string;
  collection_display_name: string;
  stable_name: string;
  rarity: string;
  wear_band: string;
  canonical_float: string;
  is_stattrak: boolean;
  is_souvenir: boolean;
  locked: boolean;
  acquired_at: string;
};
/** @deprecated Prefer ApiInventoryItem for wire data and InventoryItem for UI data. */
export type InventoryItem = ApiInventoryItem;
export type CatalogSku = { sku_id: string; item_id: string; collection_id: string; collection_slug: string; collection_display_name: string; stable_name: string; rarity: string; rarity_rank: number; wear_band: string; min_float: string; max_float: string; is_stattrak: boolean; is_souvenir: boolean; canonical_skin_id: string | null; weapon: string | null; skin_name: string | null; canonical_image_url: string | null; available_wears: string[] };
export type MarketValuation = { sku_id: string; price_microcredits: number; currency_code: 'CC'; updated_at: string; available: boolean };
export type Balance = { currency_code: 'CC'; available_microcredits: number; reserved_microcredits: number };
export type ContractHistoryItem = { contract_id: string; created_at: string; input_count: number; input_value_microcredits: number; result_display_name?: string | null; result_value_microcredits?: number | null; status: string };
const base = (import.meta.env.VITE_API_URL || 'http://127.0.0.1:8080').replace(/\/$/, '');
async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${base}/api/v1${path}`, { credentials: 'include', ...init, headers: { Accept: 'application/json', ...(init?.headers || {}) } });
  if (!response.ok) throw new Error(`API_${response.status}`);
  return response.json() as Promise<T>;
}

async function requestAllPages<T>(path: string): Promise<ApiPage<T>> {
  const items: T[] = [];
  const seenCursors = new Set<string>();
  let cursor: string | undefined;

  for (;;) {
    const query = new URLSearchParams({ limit: '200' });
    if (cursor !== undefined) query.set('cursor', cursor);
    const page = await request<ApiPage<T>>(`${path}?${query.toString()}`);
    items.push(...page.items);

    const nextCursor = page.next_cursor ?? undefined;
    if (nextCursor === undefined) return { items };
    if (seenCursors.has(nextCursor)) throw new Error('API_PAGINATION_CURSOR_REPEATED');

    seenCursors.add(nextCursor);
    cursor = nextCursor;
  }
}

export const api = {
  login: (login: string, password: string) => request<{ user_id: string }>('/auth/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ login, password }) }),
  balance: () => request<Balance>('/me/balance'),
  inventory: () => requestAllPages<ApiInventoryItem>('/me/inventory'),
  catalogSkus: () => requestAllPages<CatalogSku>('/catalog/skus'),
  marketValuations: () => requestAllPages<MarketValuation>('/market/valuations'),
  history: () => request<ApiPage<ContractHistoryItem>>('/me/history/contracts'),
  purchase: (skuId: string, idempotencyKey: string) => request<{ inventory_item_id: string }>('/me/market/purchases/' + skuId, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ idempotency_key: idempotencyKey }) }),
};

/** Join an owned item to public catalog presentation and valuation data. */
export function inventoryItemToSkin(item: ApiInventoryItem, catalog?: CatalogSku, valuation?: MarketValuation): SkinInventoryItem {
  const [stableWeapon, ...stableSkinParts] = item.stable_name.split(' | ');
  const stableSkin = stableSkinParts.join(' | ');
  const image = catalog?.canonical_image_url || null;
  return {
    id: item.item_id,
    publicId: item.item_id,
    skuId: item.sku_id,
    catalogItemId: item.catalog_item_id,
    collectionId: item.collection_id,
    collectionDisplayName: item.collection_display_name,
    weapon: catalog?.weapon?.trim() || (stableSkin ? stableWeapon : 'CS2'),
    skin: catalog?.skin_name?.trim() || stableSkin || catalog?.stable_name || item.stable_name,
    wear: item.wear_band.replace(/_/g, ' '),
    price: valuation ? valuation.price_microcredits / 1_000_000 : null,
    color: '#58d6e7',
    rarity: item.rarity,
    image: image || '',
    canonicalImageUrl: image,
    canonicalFloat: item.canonical_float,
    isStatTrak: item.is_stattrak,
    isSouvenir: item.is_souvenir,
    locked: item.locked,
    acquiredAt: item.acquired_at,
  };
}
