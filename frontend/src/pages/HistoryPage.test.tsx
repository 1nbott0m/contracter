// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { rememberContractPresentation } from '../components/ContractReveal';
import { HistoryPage } from './HistoryPage';

const session = { status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T10:00:00Z' } } as const;

beforeEach(() => sessionStorage.clear());
afterEach(cleanup);

it('renders server history with real cached flow artwork and opens the owner detail route', async () => {
  rememberContractPresentation('contract-1', {
    inputs: [{ id: 'input', weapon: 'M4A1-S', skin: 'Decimator', wear: 'Minimal Wear', price: 2, color: '#58d6e7', rarity: 'restricted', image: 'https://cdn.example.test/input.png' }],
    result: { id: 'result', weapon: 'AK-47', skin: 'Server Result', wear: 'Factory New', price: 9, color: '#b06be8', rarity: 'classified', image: 'https://cdn.example.test/result.png' },
    inputValueMicrocredits: 2_000_000,
    resultValueMicrocredits: 9_000_000,
  });
  const navigate = vi.fn();
  render(<HistoryPage session={session} navigate={navigate} client={{ history: vi.fn(async () => [{ contract_id: 'contract-1', status: 'completed', created_at: '2026-09-25T12:00:00Z', ledger_transaction_id: 42 }]) }} />);

  expect(await screen.findByRole('img', { name: 'M4A1-S | Decimator' })).toBeTruthy();
  expect(screen.getByRole('img', { name: 'AK-47 | Server Result' })).toBeTruthy();
  expect(screen.getByText('2 CC')).toBeTruthy();
  expect(screen.getByText('9 CC')).toBeTruthy();
  fireEvent.click(screen.getByRole('link', { name: /Открыть контракт contract-1/ }));
  expect(navigate).toHaveBeenCalledWith('/contracts/contract-1');
  expect(screen.queryByText('42')).toBeNull();
});

it('does not invent artwork for older summary-only history records', async () => {
  render(<HistoryPage session={session} navigate={vi.fn()} client={{ history: vi.fn(async () => [{ contract_id: 'contract-2', status: 'completed', created_at: '2026-09-25T12:00:00Z', ledger_transaction_id: null }]) }} />);

  expect(await screen.findByText('Изображения и значения недоступны в текущем API истории.')).toBeTruthy();
  expect(screen.queryByRole('img')).toBeNull();
});
