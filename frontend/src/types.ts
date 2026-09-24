/** The canonical skin shape shared by catalog, inventory, market and contract UI. */
export type SkinDefinition = {
  id: string;
  weapon: string;
  skin: string;
  wear: string;
  price: number;
  color: string;
  rarity: string;
  /** Canonical artwork URL. Never replace this with generated artwork in production. */
  image: string;
  canonicalImageUrl?: string | null;
};

export type InventoryItem = SkinDefinition & {
  publicId?: string;
  skuId?: string | number;
  state?: string;
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
  stock?: number | null;
  available?: boolean;
};

export type AsyncState<T> =
  | { status: 'idle' }
  | { status: 'loading'; data?: T }
  | { status: 'success'; data: T }
  | { status: 'empty'; data?: T }
  | { status: 'error'; error: string; data?: T };
