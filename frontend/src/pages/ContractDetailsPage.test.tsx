// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ContractDetailsPage } from './ContractDetailsPage';

afterEach(cleanup);

it('shows only the safe owner-scoped contract summary', async () => {
  render(<ContractDetailsPage
    contractId="contract-1"
    session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T10:00:00Z' } }}
    navigate={vi.fn()}
    client={{ findMyContractHistoryEntry: vi.fn(async () => ({ contract_id: 'contract-1', status: 'completed', created_at: '2026-09-25T12:00:00Z', ledger_transaction_id: 991 })) }}
  />);

  expect(await screen.findByRole('heading', { name: 'Контракт contract-1' })).toBeTruthy();
  expect(screen.getByText('Завершён')).toBeTruthy();
  expect(screen.getByText('Операция аккаунта связана')).toBeTruthy();
  expect(screen.queryByText('991')).toBeNull();
  expect(document.body.textContent).not.toMatch(/seed|signature|probability|nonce|ciphertext/i);
});

it('distinguishes an absent owner history entry from public verification', async () => {
  render(<ContractDetailsPage
    contractId="missing"
    session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T10:00:00Z' } }}
    navigate={vi.fn()}
    client={{ findMyContractHistoryEntry: vi.fn(async () => null) }}
  />);

  expect(await screen.findByText('Контракт не найден в истории этого аккаунта.')).toBeTruthy();
  expect(screen.getByText('Сводка owner-scoped истории. Она не является публичной проверкой.')).toBeTruthy();
  expect(document.body.textContent).not.toMatch(/публичная проверка выполнена|verified|fairness/i);
});
