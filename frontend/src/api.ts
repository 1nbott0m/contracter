export type ApiPage<T> = { items: T[]; next_cursor?: string | null };
export type InventoryItem = { public_id: string; sku_id: number; display_name: string; wear: string; value_microcredits: number; state: string };
export type Balance = { currency_code: 'CC'; available_microcredits: number; reserved_microcredits: number };
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
  catalogSkus: () => request<ApiPage<unknown>>('/catalog/skus'),
};
