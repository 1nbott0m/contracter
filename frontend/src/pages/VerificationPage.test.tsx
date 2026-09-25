// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { VerificationPage } from './VerificationPage';

afterEach(cleanup);

it('labels lookup as private owner history and links only a server-returned match', async () => {
  const navigate = vi.fn();
  const findMyContractHistoryEntry = vi.fn(async () => ({ contract_id: 'contract-1', status: 'completed', created_at: '2026-09-25T12:00:00Z', ledger_transaction_id: 42 }));
  render(<VerificationPage
    session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T10:00:00Z' } }}
    navigate={navigate}
    client={{ findMyContractHistoryEntry }}
  />);

  fireEvent.change(screen.getByLabelText('ID контракта'), { target: { value: 'contract-1' } });
  fireEvent.click(screen.getByRole('button', { name: 'Найти в моей истории' }));

  expect(await screen.findByText('Контракт найден в истории вашего аккаунта.')).toBeTruthy();
  expect(screen.getByText('Это не публичная или криптографическая проверка.')).toBeTruthy();
  fireEvent.click(screen.getByRole('link', { name: 'Открыть сводку' }));
  expect(navigate).toHaveBeenCalledWith('/contracts/contract-1');
});
