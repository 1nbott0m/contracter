// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ContractReveal } from './ContractReveal';
import type { InventoryItem, SkinDefinition } from '../types';

const selected: InventoryItem[] = Array.from({ length: 4 }, (_, index) => ({
  id: `input-${index}`,
  weapon: `Weapon ${index}`,
  skin: `Input ${index}`,
  wear: 'Minimal Wear',
  price: 1 + index,
  color: '#58d6e7',
  rarity: 'restricted',
  image: `https://cdn.example.test/input-${index}.png`,
}));

const result: SkinDefinition = {
  id: 'result',
  weapon: 'AK-47',
  skin: 'Server Result',
  wear: 'Factory New',
  price: 12,
  color: '#b06be8',
  rarity: 'classified',
  image: 'https://cdn.example.test/result.png',
};

beforeEach(() => {
  vi.stubGlobal('matchMedia', vi.fn(() => ({
    matches: true,
    media: '(prefers-reduced-motion: reduce)',
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it('uses the selected canonical artwork and withholds the result until the server succeeds', () => {
  const { rerender } = render(<ContractReveal open selected={selected} state={{ status: 'submitting' }} onClose={vi.fn()} onNew={vi.fn()} />);

  expect(screen.getByRole('dialog', { name: 'Фиксация контракта' })).toBeTruthy();
  expect(screen.getAllByRole('img').map((image) => image.getAttribute('src'))).toEqual(selected.map((item) => item.image));
  expect(screen.queryByText('Server Result')).toBeNull();

  rerender(<ContractReveal open selected={selected} state={{ status: 'success', contractId: 'contract-1', result }} onClose={vi.fn()} onNew={vi.fn()} />);
  expect(screen.getByText('Server Result')).toBeTruthy();
  expect(screen.getByRole('img', { name: 'AK-47 | Server Result' }).getAttribute('src')).toBe(result.image);
  expect(screen.getByText(/contract-1/)).toBeTruthy();
});

it('renders a truthful error state without a fabricated result', () => {
  render(<ContractReveal open selected={selected} state={{ status: 'error' }} onClose={vi.fn()} onNew={vi.fn()} />);

  expect(screen.getByRole('alert').textContent).toContain('Контракт не подтверждён');
  expect(screen.queryByText('Server Result')).toBeNull();
});
