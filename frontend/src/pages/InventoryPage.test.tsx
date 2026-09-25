// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ApiInventoryItem, CatalogSku, MarketValuation } from '../api';
import { CONTRACT_SELECTION_STORAGE_KEY, InventoryPage } from './InventoryPage';

const inventory: ApiInventoryItem[] = [
  { item_id: 'owned-slate', sku_id: 'sku-slate', catalog_item_id: 'item-slate', collection_id: 'collection', collection_display_name: 'Collection', stable_name: 'AK-47 | Slate', rarity: 'restricted', wear_band: 'minimal_wear', canonical_float: '0.10', is_stattrak: false, is_souvenir: false, locked: false, acquired_at: '2026-09-25T12:00:00Z' },
  { item_id: 'owned-fade', sku_id: 'sku-fade', catalog_item_id: 'item-fade', collection_id: 'collection', collection_display_name: 'Collection', stable_name: 'Glock-18 | Fade', rarity: 'classified', wear_band: 'factory_new', canonical_float: '0.03', is_stattrak: false, is_souvenir: false, locked: false, acquired_at: '2026-09-25T13:00:00Z' },
];

const catalog: CatalogSku[] = inventory.map((item, index) => ({
  sku_id: item.sku_id, item_id: item.catalog_item_id, collection_id: item.collection_id, collection_slug: 'collection', collection_display_name: 'Collection', stable_name: item.stable_name,
  rarity: item.rarity, rarity_rank: index + 3, wear_band: item.wear_band, min_float: '0', max_float: '1', is_stattrak: false, is_souvenir: false,
  canonical_skin_id: item.item_id, weapon: index === 0 ? 'AK-47' : 'Glock-18', skin_name: index === 0 ? 'Slate' : 'Fade', canonical_image_url: `https://cdn.example.test/${item.item_id}.png`, available_wears: [item.wear_band],
}));

const valuations: MarketValuation[] = [
  { sku_id: 'sku-slate', price_microcredits: 2_000_000, currency_code: 'CC', updated_at: '2026-09-25T12:00:00Z', available: true },
  { sku_id: 'sku-fade', price_microcredits: 9_000_000, currency_code: 'CC', updated_at: '2026-09-25T12:00:00Z', available: true },
];

beforeEach(() => sessionStorage.clear());
afterEach(cleanup);

it('filters owned items and persists an add-to-contract selection', async () => {
  const client = {
    inventory: vi.fn(async () => ({ items: inventory })),
    catalogSkus: vi.fn(async () => ({ items: catalog })),
    marketValuations: vi.fn(async () => ({ items: valuations })),
  };
  render(<InventoryPage session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T12:00:00Z' } }} navigate={vi.fn()} client={client} />);

  expect(await screen.findByText('Slate')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Оружие'), { target: { value: 'AK-47' } });
  fireEvent.change(screen.getByLabelText('Цена до'), { target: { value: '3' } });
  fireEvent.change(screen.getByPlaceholderText('Название скина'), { target: { value: 'slate' } });
  fireEvent.click(screen.getByRole('button', { name: 'Очистить поиск инвентаря' }));

  expect(screen.getByText('Slate')).toBeTruthy();
  expect(screen.queryByText('Fade')).toBeNull();

  fireEvent.click(screen.getByRole('button', { name: 'Добавить AK-47 | Slate в контракт' }));
  expect(screen.getByText('AK-47 | Slate добавлен в контракт.')).toBeTruthy();
  expect(JSON.parse(sessionStorage.getItem(CONTRACT_SELECTION_STORAGE_KEY) || '[]')).toEqual(['owned-slate']);
});

it('renders locked inventory as unavailable for contract selection', async () => {
  const client = {
    inventory: vi.fn(async () => ({ items: [{ ...inventory[0], locked: true }] })),
    catalogSkus: vi.fn(async () => ({ items: catalog.slice(0, 1) })),
    marketValuations: vi.fn(async () => ({ items: valuations.slice(0, 1) })),
  };
  render(<InventoryPage session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T12:00:00Z' } }} navigate={vi.fn()} client={client} />);

  expect((await screen.findByRole('button', { name: 'AK-47 | Slate недоступен: предмет заблокирован' }) as HTMLButtonElement).disabled).toBe(true);
});
