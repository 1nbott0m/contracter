// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { ApiInventoryItem, CatalogSku, MarketValuation, QuoteResponse } from '../api';
import { CONTRACT_SELECTION_STORAGE_KEY } from './InventoryPage';
import { ContractsPage } from './ContractsPage';

const inventory: ApiInventoryItem[] = Array.from({ length: 4 }, (_, index) => ({
  item_id: `item-${index + 1}`,
  sku_id: `sku-${index + 1}`,
  catalog_item_id: `catalog-${index + 1}`,
  collection_id: 'collection',
  collection_display_name: 'Collection',
  stable_name: `Weapon ${index + 1} | Skin ${index + 1}`,
  rarity: 'restricted',
  wear_band: 'minimal_wear',
  canonical_float: '0.12345678',
  is_stattrak: false,
  is_souvenir: false,
  locked: false,
  acquired_at: '2026-09-24T12:00:00Z',
}));

const catalog: CatalogSku[] = inventory.map((item, index) => ({
  sku_id: item.sku_id,
  item_id: item.catalog_item_id,
  collection_id: item.collection_id,
  collection_slug: 'collection',
  collection_display_name: item.collection_display_name,
  stable_name: item.stable_name,
  rarity: item.rarity,
  rarity_rank: 3,
  wear_band: item.wear_band,
  min_float: '0.07',
  max_float: '0.15',
  is_stattrak: false,
  is_souvenir: false,
  canonical_skin_id: `skin-${index + 1}`,
  weapon: `Weapon ${index + 1}`,
  skin_name: `Skin ${index + 1}`,
  canonical_image_url: `https://cdn.example.test/${index + 1}.png`,
  available_wears: ['minimal_wear'],
}));

const valuations: MarketValuation[] = inventory.map((item) => ({
  sku_id: item.sku_id,
  price_microcredits: 1_000_000,
  currency_code: 'CC',
  updated_at: '2026-09-24T12:00:00Z',
  available: true,
}));

const quote: QuoteResponse = {
  quote_id: 'quote-id',
  formula_version: 'v1',
  input_value_microcredits: 4_000_000,
  expected_buyback_microcredits: 3_500_000,
  total_microcredits: 3_600_000,
  currency_code: 'CC',
  expires_at: '2026-09-24T12:10:00Z',
  inputs: [],
  outcomes: [],
};

afterEach(() => {
  cleanup();
  sessionStorage.clear();
});

it('restores inventory items added to the contract from the inventory route', async () => {
  sessionStorage.setItem(CONTRACT_SELECTION_STORAGE_KEY, JSON.stringify(['item-2']));
  const client = {
    inventory: vi.fn(async () => ({ items: inventory })),
    catalogSkus: vi.fn(async () => ({ items: catalog })),
    marketValuations: vi.fn(async () => ({ items: valuations })),
    allocateQuote: vi.fn(), createQuote: vi.fn(), acceptQuote: vi.fn(), history: vi.fn(),
  };

  render(<ContractsPage setApiStatus={vi.fn()} client={client} />);

  await waitFor(() => expect(screen.getByText('1 / 10 ПРЕДМЕТОВ')).toBeTruthy());
  expect(screen.getAllByRole('button', { name: 'Убрать Weapon 2 | Skin 2 из контракта' }).length).toBeGreaterThan(0);
});

it('does not replay an ambiguous quote create against a stale allocation', async () => {
  const createQuote = vi.fn()
    .mockRejectedValueOnce(new Error('transport failed'))
    .mockResolvedValueOnce(quote);
  const acceptQuote = vi.fn(async (_quoteId: string, _idempotencyKey: string) => ({ contract_id: 'contract-id' }));
  const client = {
    inventory: vi.fn(async () => ({ items: inventory })),
    catalogSkus: vi.fn(async () => ({ items: catalog })),
    marketValuations: vi.fn(async () => ({ items: valuations })),
    allocateQuote: vi.fn(async () => ({ allocation_id: 'allocation-id', commitment: [] })),
    createQuote,
    acceptQuote,
    history: vi.fn(async () => []),
  };
  render(<ContractsPage setApiStatus={vi.fn()} client={client} />);

  for (const item of inventory) {
    fireEvent.click(await screen.findByRole('button', { name: `Выбрать ${item.stable_name} для контракта` }));
  }
  fireEvent.click(screen.getByRole('button', { name: 'ЗАКЛЮЧИТЬ КОНТРАКТ' }));
  expect(await screen.findByRole('alert')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'ПОВТОРИТЬ' }));

  expect(await screen.findByText('Контракт contract-id принят сервером.')).toBeTruthy();
  expect(client.allocateQuote).toHaveBeenCalledTimes(2);
  expect(createQuote).toHaveBeenCalledTimes(2);
  expect(createQuote.mock.calls[0]?.[0]).toBe('allocation-id');
  expect(createQuote.mock.calls[1]?.[0]).toBe('allocation-id');
  expect(createQuote.mock.calls[0]?.[2]).not.toBe(createQuote.mock.calls[1]?.[2]);
  expect(acceptQuote).toHaveBeenCalledTimes(1);
});

it('does not infer accept success from unrelated owner history', async () => {
  const history = vi.fn()
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([{ contract_id: 'reconciled-contract', status: 'completed', created_at: '2026-09-24T12:01:00Z', ledger_transaction_id: 42 }]);
  const client = {
    inventory: vi.fn(async () => ({ items: inventory })),
    catalogSkus: vi.fn(async () => ({ items: catalog })),
    marketValuations: vi.fn(async () => ({ items: valuations })),
    allocateQuote: vi.fn(async () => ({ allocation_id: 'allocation-id', commitment: [] })),
    createQuote: vi.fn(async () => quote),
    acceptQuote: vi.fn(async (_quoteId: string, _idempotencyKey: string) => { throw new TypeError('network failed after send'); }),
    history,
  };
  render(<ContractsPage setApiStatus={vi.fn()} client={client} />);

  for (const item of inventory) {
    fireEvent.click(await screen.findByRole('button', { name: `Выбрать ${item.stable_name} для контракта` }));
  }
  fireEvent.click(screen.getByRole('button', { name: 'ЗАКЛЮЧИТЬ КОНТРАКТ' }));

  expect(await screen.findByRole('alert')).toBeTruthy();
  expect(history).toHaveBeenCalledTimes(0);
  expect(client.acceptQuote).toHaveBeenCalledTimes(1);
  expect(client.acceptQuote.mock.calls[0]?.[0]).toBe('quote-id');
  expect(client.acceptQuote.mock.calls[0]?.[1]).toEqual(expect.any(String));
});
