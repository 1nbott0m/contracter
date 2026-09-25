// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { CatalogSku } from '../api';
import { ContractBuilder } from '../components/ContractBuilder';
import { GlobalSearch } from '../components/GlobalSearch';
import { ProfileMenu } from '../components/ProfileMenu';
import type { InventoryItem } from '../types';

const catalog: CatalogSku[] = [{
  sku_id: 'sku-1', item_id: 'item-1', collection_id: 'collection', collection_slug: 'collection', collection_display_name: 'Collection', stable_name: 'AK-47 | Slate',
  rarity: 'restricted', rarity_rank: 3, wear_band: 'minimal_wear', min_float: '0.07', max_float: '0.15', is_stattrak: false, is_souvenir: false,
  canonical_skin_id: 'skin-1', weapon: 'AK-47', skin_name: 'Slate', canonical_image_url: 'https://cdn.example.test/slate.png', available_wears: ['minimal_wear'],
}];

const inventory: InventoryItem[] = Array.from({ length: 4 }, (_, index) => ({
  id: `item-${index}`,
  weapon: `Weapon ${index}`,
  skin: `Skin ${index}`,
  wear: 'Minimal Wear',
  price: index + 1,
  color: '#58d6e7',
  rarity: 'restricted',
  image: `https://cdn.example.test/item-${index}.png`,
}));

afterEach(cleanup);

it('closes global search from anywhere on Escape and restores its trigger focus', async () => {
  render(<GlobalSearch navigate={vi.fn()} client={{ catalogSkus: vi.fn(async () => ({ items: catalog })) }} />);
  const trigger = screen.getByRole('button', { name: 'Открыть глобальный поиск' });

  fireEvent.click(trigger);
  expect(await screen.findByRole('dialog', { name: 'Глобальный поиск' })).toBeTruthy();
  fireEvent.keyDown(window, { key: 'Escape' });

  expect(screen.queryByRole('dialog', { name: 'Глобальный поиск' })).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});

it('closes the profile menu on Escape and restores its trigger focus', async () => {
  render(<ProfileMenu session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T12:00:00Z' } }} navigate={vi.fn()} logout={vi.fn()} />);
  const trigger = screen.getByRole('button', { name: 'Открыть меню профиля' });

  fireEvent.click(trigger);
  expect(screen.getByRole('menu')).toBeTruthy();
  fireEvent.keyDown(window, { key: 'Escape' });

  expect(screen.queryByRole('menu')).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});

it('closes the item inspector on Escape and restores its trigger focus', async () => {
  render(<ContractBuilder items={inventory} initialSelected={inventory} onSubmit={vi.fn()} />);
  const trigger = screen.getAllByRole('button', { name: 'Подробнее о Weapon 0 | Skin 0' })[0]!;

  fireEvent.click(trigger);
  expect(screen.getByRole('dialog', { name: 'Weapon 0 | Skin 0' })).toBeTruthy();
  fireEvent.keyDown(window, { key: 'Escape' });

  expect(screen.queryByRole('dialog', { name: 'Weapon 0 | Skin 0' })).toBeNull();
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});
