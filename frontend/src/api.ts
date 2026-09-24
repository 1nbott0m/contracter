export type ApiPage<T> = { items: T[]; next_cursor?: string | null };
export type InventoryItem = { public_id: string; sku_id: number; display_name: string; wear: string; value_microcredits: number; state: string };
export type CatalogSku = { sku_id: string; item_id: string; collection_id: string; collection_slug: string; collection_display_name: string; stable_name: string; rarity: string; rarity_rank: number; wear_band: string; min_float: string; max_float: string; canonical_skin_id: string | null; weapon: string | null; skin_name: string | null; canonical_image_url: string | null; available_wears: string[] };
export type Balance = { currency_code: 'CC'; available_microcredits: number; reserved_microcredits: number };
export type ContractHistoryItem = { contract_id: string; created_at: string; input_count: number; input_value_microcredits: number; result_display_name?: string | null; result_value_microcredits?: number | null; status: string };
const base = (import.meta.env.VITE_API_URL || 'http://127.0.0.1:8080').replace(/\/$/, '');
async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${base}/api/v1${path}`, { credentials: 'include', ...init, headers: { Accept: 'application/json', ...(init?.headers || {}) } });
  if (!response.ok) throw new Error(`API_${response.status}`);
  return response.json() as Promise<T>;
}
export const api = {
  login: (login: string, password: string) => request<{ user_id: string }>('/auth/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ login, password }) }),
  balance: () => request<Balance>('/me/balance'),
  inventory: () => request<ApiPage<InventoryItem>>('/me/inventory'),
  catalogSkus: () => request<ApiPage<CatalogSku>>('/catalog/skus'),
  history: () => request<ApiPage<ContractHistoryItem>>('/me/history/contracts'),
  purchase: (skuId: string, idempotencyKey: string) => request<{ inventory_item_id: string }>('/me/market/purchases/' + skuId, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ idempotency_key: idempotencyKey }) }),
};
