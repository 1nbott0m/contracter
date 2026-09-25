/** The canonical skin shape shared by catalog, inventory, market and contract UI. */
export type SkinDefinition = {
  id: string;
  weapon: string;
  skin: string;
  wear: string;
  /** Display valuation in CC, or null when no current valuation exists. */
  price: number | null;
  color: string;
  rarity: string;
  /** Canonical artwork URL. Never replace this with generated artwork in production. */
  image: string;
  canonicalImageUrl?: string | null;
};

export type InventoryItem = SkinDefinition & {
  publicId?: string;
  skuId?: string | number;
  catalogItemId?: string;
  collectionId?: string;
  collectionDisplayName?: string;
  canonicalFloat?: string;
  isStatTrak?: boolean;
  isSouvenir?: boolean;
  locked?: boolean;
  acquiredAt?: string;
};

export type HistoryItem = {
  contractId: string;
  createdAt: string;
  inputCount: number;
  inputValueMicrocredits: number;
  resultDisplayName?: string | null;
  resultValueMicrocredits?: number | null;
  result?: SkinDefinition | null;
  status: string;
};

export type MarketItem = SkinDefinition & {
  skuId: string;
  stock?: number | null;
  available?: boolean;
  valuationUpdatedAt?: string | null;
};

export type InventoryFilterValues = {
  search: string;
  weapon: string;
  rarity: string;
  wear: string;
  minPrice: string;
  maxPrice: string;
  sort: 'newest' | 'name' | 'price-asc' | 'price-desc';
};

export type AsyncState<T> =
  | { status: 'idle' }
  | { status: 'loading'; data?: T }
  | { status: 'success'; data: T }
  | { status: 'empty'; data?: T }
  | { status: 'error'; error: string; data?: T };
