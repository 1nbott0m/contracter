// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { rememberContractPresentation } from './ContractReveal';
import { LiveActivity } from './LiveActivity';

afterEach(() => {
  cleanup();
  sessionStorage.clear();
});

const account = { status: 'authenticated', account: { user_id: 'user-1', login: 'tester', created_at: '2026-09-25T08:00:00Z' } } as const;

it('reports loading and connected states without inventing an online count', async () => {
  let resolveHealth!: (value: { status: 'live' }) => void;
  const health = new Promise<{ status: 'live' }>((resolve) => { resolveHealth = resolve; });
  render(<LiveActivity session={{ status: 'unauthenticated' }} client={{ serviceHealth: vi.fn(() => health), history: vi.fn() }} />);

  expect(screen.getByText('ONLINE: ПРОВЕРКА')).toBeTruthy();
  expect(screen.queryByText(/\d+ ONLINE/)).toBeNull();

  resolveHealth({ status: 'live' });
  await waitFor(() => expect(screen.getByText('ONLINE: ПОДКЛЮЧЕНО')).toBeTruthy());
  expect(screen.getByText(/Публичная лента операций API не предоставляется/)).toBeTruthy();
});

it('shows unavailable and reconnecting states when connectivity fails and is retried', async () => {
  const serviceHealth = vi.fn()
    .mockRejectedValueOnce(new Error('offline'))
    .mockResolvedValueOnce({ status: 'live' });
  render(<LiveActivity session={{ status: 'unauthenticated' }} client={{ serviceHealth, history: vi.fn() }} />);

  await waitFor(() => expect(screen.getByText('ONLINE: НЕДОСТУПНО')).toBeTruthy());
  fireEvent.click(screen.getByRole('button', { name: 'Повторить подключение' }));
  expect(screen.getByText('ONLINE: ПЕРЕПОДКЛЮЧЕНИЕ')).toBeTruthy();
  await waitFor(() => expect(screen.getByText('ONLINE: ПОДКЛЮЧЕНО')).toBeTruthy());
});

it('renders canonical result artwork only for a real owner-history entry with stored server presentation', async () => {
  rememberContractPresentation('contract-1', {
    inputs: [],
    result: {
      id: 'skin-1', weapon: 'AK-47', skin: 'Slate', wear: 'Minimal Wear', price: 4.2,
      color: '#58d6e7', rarity: 'restricted', image: 'https://cdn.example.test/slate.png',
      canonicalImageUrl: 'https://cdn.example.test/slate.png',
    },
    inputValueMicrocredits: 4_000_000,
    resultValueMicrocredits: 4_200_000,
  });
  render(<LiveActivity session={account} client={{
    serviceHealth: vi.fn(async () => ({ status: 'live' as const })),
    history: vi.fn(async () => [{ contract_id: 'contract-1', status: 'completed', created_at: '2026-09-25T08:00:00Z', ledger_transaction_id: 42 }]),
  }} />);

  const image = await screen.findByRole('img', { name: 'AK-47 | Slate' });
  expect(image.getAttribute('src')).toBe('https://cdn.example.test/slate.png');
  expect(screen.getByText('contract-1')).toBeTruthy();
});

it('labels an empty authenticated history instead of substituting demo activity', async () => {
  render(<LiveActivity session={account} client={{
    serviceHealth: vi.fn(async () => ({ status: 'live' as const })),
    history: vi.fn(async () => []),
  }} />);

  expect(await screen.findByText('В истории аккаунта пока нет подтверждённых операций')).toBeTruthy();
  expect(screen.queryByRole('img')).toBeNull();
});
