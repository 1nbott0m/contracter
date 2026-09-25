// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { CatalogSku } from '../api';
import { GlobalSearch } from './GlobalSearch';

const catalog: CatalogSku[] = ['Slate', 'Neon Rider'].map((skin, index) => ({
  sku_id: `sku-${index}`, item_id: `item-${index}`, collection_id: 'collection', collection_slug: 'collection', collection_display_name: 'Collection', stable_name: `AK-47 | ${skin}`,
  rarity: 'restricted', rarity_rank: 3, wear_band: 'minimal_wear', min_float: '0.07', max_float: '0.15', is_stattrak: false, is_souvenir: false,
  canonical_skin_id: `skin-${index}`, weapon: 'AK-47', skin_name: skin, canonical_image_url: `https://cdn.example.test/${index}.png`, available_wears: ['minimal_wear'],
}));

afterEach(cleanup);

it('supports Arrow navigation, Enter selection and Escape dismissal', async () => {
  const navigate = vi.fn();
  render(<GlobalSearch navigate={navigate} client={{ catalogSkus: vi.fn(async () => ({ items: catalog })) }} />);

  fireEvent.click(screen.getByRole('button', { name: 'Открыть глобальный поиск' }));
  const input = await screen.findByRole('searchbox', { name: 'Поиск по маркету' });
  fireEvent.change(input, { target: { value: 'AK-47' } });
  fireEvent.keyDown(input, { key: 'ArrowDown' });
  fireEvent.keyDown(input, { key: 'Enter' });

  expect(navigate).toHaveBeenCalledWith('/market?search=AK-47%20%7C%20Neon%20Rider');
  expect(screen.queryByRole('dialog', { name: 'Глобальный поиск' })).toBeNull();

  fireEvent.click(screen.getByRole('button', { name: 'Открыть глобальный поиск' }));
  expect(await screen.findByRole('dialog', { name: 'Глобальный поиск' })).toBeTruthy();
  fireEvent.keyDown(screen.getByRole('searchbox', { name: 'Поиск по маркету' }), { key: 'Escape' });
  expect(screen.queryByRole('dialog', { name: 'Глобальный поиск' })).toBeNull();
});
