import type { InventoryItem as SkinInventoryItem } from './types';

export type ApiPage<T> = { items: T[]; next_cursor?: string | null };
export type ApiErrorEnvelope = {
  error?: {
    code?: string;
    message?: string;
    request_id?: string;
  };
};

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: string;
  readonly requestId?: string;

  constructor(status: number, code: string, message: string, requestId?: string) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}

export type Account = { user_id: string; login: string; created_at: string };
export type LoginResponse = { user_id: string };
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
export type Balance = { currency_code: 'CC'; balance_microcredits: number };
export type ContractHistoryItem = {
  contract_id: string;
  status: string;
  created_at: string;
  ledger_transaction_id: number | null;
};
export type MarketPurchaseResponse = {
  operation_id: string;
  inventory_item_id: string;
  amount_microcredits: number;
  currency_code: 'CC';
};
export type QuoteAllocationResponse = {
  allocation_id: string;
  commitment: number[];
};
export type QuoteInput = {
  position: number;
  item_id: string;
  canonical_float: string;
};
export type QuoteOutcome = {
  position: number;
  item_id: string;
  output_float: string;
  probability_numerator: number;
  probability_denominator: number;
  buyback_microcredits: number;
  currency_code: 'CC';
};
export type QuoteResponse = {
  quote_id: string;
  formula_version: string;
  input_value_microcredits: number;
  expected_buyback_microcredits: number;
  total_microcredits: number;
  currency_code: 'CC';
  expires_at: string;
  inputs: QuoteInput[];
  outcomes: QuoteOutcome[];
};
export type AcceptQuoteResponse = { contract_id: string };

type RuntimeEnvironment = {
  DEV: boolean;
  VITE_ENABLE_DEV_FALLBACK?: string;
};

/** Development fixtures require both Vite development mode and an explicit opt-in. */
export function isDevelopmentFallbackEnabled(
  environment: RuntimeEnvironment = import.meta.env,
): boolean {
  return environment.DEV && environment.VITE_ENABLE_DEV_FALLBACK === 'true';
}

const base = (import.meta.env.VITE_API_URL || 'http://127.0.0.1:8080').replace(/\/$/, '');

async function apiFailure(response: Response): Promise<ApiRequestError> {
  let envelope: ApiErrorEnvelope | undefined;
  try {
    envelope = await response.json() as ApiErrorEnvelope;
  } catch {
    // A proxy can return HTML or an empty body. The HTTP status remains useful.
  }

  return new ApiRequestError(
    response.status,
    envelope?.error?.code || `HTTP_${response.status}`,
    envelope?.error?.message || `API request failed with status ${response.status}`,
    envelope?.error?.request_id,
  );
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set('Accept', 'application/json');
  const response = await fetch(`${base}/api/v1${path}`, {
    ...init,
    credentials: 'include',
    headers,
  });
  if (!response.ok) throw await apiFailure(response);
  if (response.status === 204) return undefined as T;
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

async function requestAllContractHistory(): Promise<ContractHistoryItem[]> {
  const items: ContractHistoryItem[] = [];
  const seenCursors = new Set<string>();
  let cursor: string | undefined;

  for (;;) {
    const query = new URLSearchParams({ limit: '200' });
    if (cursor !== undefined) query.set('cursor', cursor);
    const page = await request<ContractHistoryItem[]>(`/me/history/contracts?${query.toString()}`);
    items.push(...page);
    if (page.length < 200) return items;

    const nextCursor = page[page.length - 1]?.contract_id;
    if (!nextCursor || seenCursors.has(nextCursor)) {
      throw new Error('API_PAGINATION_CURSOR_REPEATED');
    }
    seenCursors.add(nextCursor);
    cursor = nextCursor;
  }
}

async function findContract(contractId: string): Promise<ContractHistoryItem | null> {
  const normalizedId = contractId.trim().toLowerCase();
  const history = await requestAllContractHistory();
  return history.find((item) => item.contract_id.toLowerCase() === normalizedId) ?? null;
}

const marketPurchase = (skuId: string, idempotencyKey: string) => request<MarketPurchaseResponse>(
  `/me/market/purchases/${encodeURIComponent(skuId)}`,
  {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ idempotency_key: idempotencyKey }),
  },
);

const jsonPost = <T>(path: string, body?: unknown) => request<T>(path, {
  method: 'POST',
  headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
  body: body === undefined ? undefined : JSON.stringify(body),
});

export const api = {
  login: (login: string, password: string) => request<LoginResponse>('/auth/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ login, password }) }),
  logout: () => request<void>('/auth/logout', { method: 'POST' }),
  me: () => request<Account>('/me'),
  balance: () => request<Balance>('/me/balance'),
  inventory: () => requestAllPages<ApiInventoryItem>('/me/inventory'),
  catalogSkus: () => requestAllPages<CatalogSku>('/catalog/skus'),
  marketValuations: () => requestAllPages<MarketValuation>('/market/valuations'),
  allocateQuote: () => jsonPost<QuoteAllocationResponse>('/me/quote-allocations'),
  createQuote: (allocationId: string, itemIds: string[], clientSeed: string) => jsonPost<QuoteResponse>('/me/quotes', {
    allocation_id: allocationId,
    item_ids: itemIds,
    client_seed: clientSeed,
  }),
  acceptQuote: (quoteId: string, idempotencyKey: string) => jsonPost<AcceptQuoteResponse>(
    `/me/quote/${encodeURIComponent(quoteId)}/accept`,
    { idempotency_key: idempotencyKey },
  ),
  history: requestAllContractHistory,
  findMyContractHistoryEntry: findContract,
  marketPurchase,
  /** @deprecated Prefer marketPurchase. */
  purchase: marketPurchase,
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
