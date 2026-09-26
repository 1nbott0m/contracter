// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { CatalogSku, MarketValuation } from '../api';
import { MarketPage } from './MarketPage';

const catalog: CatalogSku[] = [
  {
    sku_id: 'sku-slate', item_id: 'item-slate', collection_id: 'collection', collection_slug: 'collection',
    collection_display_name: 'Collection', stable_name: 'AK-47 | Slate', rarity: 'restricted', rarity_rank: 3,
    wear_band: 'minimal_wear', min_float: '0.07', max_float: '0.15', is_stattrak: false, is_souvenir: false,
    canonical_skin_id: 'ak47-slate', weapon: 'AK-47', skin_name: 'Slate',
    canonical_image_url: 'https://cdn.example.test/slate.png', available_wears: ['minimal_wear'],
  },
  {
    sku_id: 'sku-fade', item_id: 'item-fade', collection_id: 'collection', collection_slug: 'collection',
    collection_display_name: 'Collection', stable_name: 'Glock-18 | Fade', rarity: 'classified', rarity_rank: 4,
    wear_band: 'factory_new', min_float: '0.00', max_float: '0.07', is_stattrak: false, is_souvenir: false,
    canonical_skin_id: 'glock-fade', weapon: 'Glock-18', skin_name: 'Fade',
    canonical_image_url: 'https://cdn.example.test/fade.png', available_wears: ['factory_new'],
  },
];

const valuations: MarketValuation[] = [
  { sku_id: 'sku-slate', price_microcredits: 2_000_000, currency_code: 'CC', updated_at: '2026-09-25T12:00:00Z', available: true },
  { sku_id: 'sku-fade', price_microcredits: 9_000_000, currency_code: 'CC', updated_at: '2026-09-25T12:00:00Z', available: false },
];

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

afterEach(cleanup);

it('filters market results and keeps unavailable offers unpurchasable', async () => {
  const client = {
    catalogSkusPage: vi.fn(async () => ({ items: catalog })),
    marketValuations: vi.fn(async () => ({ items: valuations })),
    balance: vi.fn(async () => ({ currency_code: 'CC' as const, balance_microcredits: 12_000_000 })),
    inventory: vi.fn(async () => ({ items: [] })),
    marketPurchase: vi.fn(),
  };
  render(<MarketPage session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T12:00:00Z' } }} navigate={vi.fn()} client={client} />);

  expect(await screen.findByText('Slate')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Поиск по маркету'), { target: { value: 'fade' } });

  expect(screen.queryByText('Slate')).toBeNull();
  expect(screen.getByText('Fade')).toBeTruthy();
  expect((screen.getByRole('button', { name: 'Glock-18 | Fade недоступен для покупки' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Очистить поиск маркета' }));
  expect(screen.getByText('Slate')).toBeTruthy();
});

it('uses one idempotency key while a purchase is retried and refreshes balance and inventory after confirmation', async () => {
  const pending = deferred<{ operation_id: string; inventory_item_id: string; amount_microcredits: number; currency_code: 'CC' }>();
  const balance = vi.fn()
    .mockResolvedValueOnce({ currency_code: 'CC', balance_microcredits: 12_000_000 })
    .mockResolvedValueOnce({ currency_code: 'CC', balance_microcredits: 10_000_000 });
  const onBalanceChange = vi.fn();
  const client = {
    catalogSkusPage: vi.fn(async () => ({ items: catalog.slice(0, 1) })),
    marketValuations: vi.fn(async () => ({ items: valuations.slice(0, 1) })),
    balance,
    inventory: vi.fn(async () => ({ items: [{ item_id: 'owned-item' }] })),
    marketPurchase: vi.fn(() => pending.promise),
  };
  render(<MarketPage session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T12:00:00Z' } }} navigate={vi.fn()} client={client as never} onBalanceChange={onBalanceChange} />);

  fireEvent.click(await screen.findByRole('button', { name: 'Купить AK-47 | Slate за 2 CC' }));
  expect(screen.getByRole('button', { name: 'Покупка AK-47 | Slate выполняется' })).toBeTruthy();
  expect(client.marketPurchase).toHaveBeenCalledWith('sku-slate', expect.any(String));

  pending.resolve({ operation_id: 'operation', inventory_item_id: 'owned-item', amount_microcredits: 2_000_000, currency_code: 'CC' });
  expect(await screen.findByText('Покупка подтверждена. Баланс и инвентарь обновлены.')).toBeTruthy();
  expect(balance).toHaveBeenCalledTimes(2);
  expect(client.inventory).toHaveBeenCalledTimes(1);
  await waitFor(() => expect(onBalanceChange).toHaveBeenLastCalledWith(10_000_000));
});

it('explains when the server catalog is ready but CC prices are not published', async () => {
  const client = {
    catalogSkusPage: vi.fn(async () => ({ items: catalog.slice(0, 1) })),
    marketValuations: vi.fn(async () => ({ items: [] })),
    balance: vi.fn(),
    inventory: vi.fn(),
    marketPurchase: vi.fn(),
  };
  render(<MarketPage session={{ status: 'unauthenticated' }} navigate={vi.fn()} client={client} />);

  expect(await screen.findByText('Цены CC ещё не опубликованы')).toBeTruthy();
  expect(screen.getByText('Каталог уже загружен с сервера. Покупки включатся после публикации подтверждённых оценок.')).toBeTruthy();
});

it('renders the first catalog page before following the next cursor', async () => {
  const nextPage = catalog.map((item) => ({ ...item, sku_id: `${item.sku_id}-next`, item_id: `${item.item_id}-next` }));
  const catalogSkusPage = vi.fn()
    .mockResolvedValueOnce({ items: catalog.slice(0, 1), next_cursor: 'next-page' })
    .mockResolvedValueOnce({ items: nextPage });
  const client = {
    catalogSkusPage,
    marketValuations: vi.fn(async () => ({ items: valuations.slice(0, 1) })),
    balance: vi.fn(),
    inventory: vi.fn(),
    marketPurchase: vi.fn(),
  };
  render(<MarketPage session={{ status: 'unauthenticated' }} navigate={vi.fn()} client={client} />);

  expect(await screen.findByText('Slate')).toBeTruthy();
  expect(catalogSkusPage).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: 'ЗАГРУЗИТЬ ЕЩЁ' }));
  expect(await screen.findByText('Fade')).toBeTruthy();
  expect(catalogSkusPage).toHaveBeenLastCalledWith('next-page');
});
