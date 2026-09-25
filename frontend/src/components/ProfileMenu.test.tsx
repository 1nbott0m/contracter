// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ProfileMenu } from './ProfileMenu';

afterEach(cleanup);

it('routes unauthenticated users to login', () => {
  const navigate = vi.fn();
  render(<ProfileMenu session={{ status: 'unauthenticated' }} navigate={navigate} logout={vi.fn()} />);

  fireEvent.click(screen.getByRole('button', { name: 'Войти' }));
  expect(navigate).toHaveBeenCalledWith('/login');
});

it('offers authenticated shortcuts and completes logout', async () => {
  const navigate = vi.fn();
  const logout = vi.fn(async () => true);
  render(<ProfileMenu session={{ status: 'authenticated', account: { user_id: 'user', login: 'tester', created_at: '2026-09-25T12:00:00Z' } }} navigate={navigate} logout={logout} />);

  fireEvent.click(screen.getByRole('button', { name: 'Открыть меню профиля' }));
  fireEvent.click(screen.getByRole('menuitem', { name: 'Инвентарь' }));
  expect(navigate).toHaveBeenCalledWith('/inventory');

  fireEvent.click(screen.getByRole('button', { name: 'Открыть меню профиля' }));
  fireEvent.click(screen.getByRole('menuitem', { name: 'Выйти' }));
  await waitFor(() => expect(logout).toHaveBeenCalledTimes(1));
  expect(navigate).toHaveBeenCalledWith('/login', { replace: true });
});
