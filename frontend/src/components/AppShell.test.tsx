// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { AppShell } from './AppShell';
import { matchRoute } from '../router';

afterEach(cleanup);

it('renders shared chrome and marks the current primary route', () => {
  render(
    <AppShell route={matchRoute('/inventory')} navigate={vi.fn()} apiStatus="live">
      <h1>Инвентарь пользователя</h1>
    </AppShell>,
  );

  expect(screen.getByText('Инвентарь пользователя')).toBeTruthy();
  expect(screen.getByRole('link', { name: 'ИНВЕНТАРЬ' }).getAttribute('aria-current')).toBe('page');
  expect(screen.getByText(/API CONNECTED/)).toBeTruthy();
  expect(screen.getByRole('contentinfo')).toBeTruthy();
  expect(screen.getByRole('navigation', { name: 'Основная навигация' })).toBeTruthy();
  expect(screen.getByRole('navigation', { name: 'Мобильная навигация' })).toBeTruthy();
});

it('uses client-side navigation for ordinary shell links', () => {
  const navigate = vi.fn();
  render(
    <AppShell route={matchRoute('/contracts')} navigate={navigate} apiStatus="fallback">
      <span>Контент</span>
    </AppShell>,
  );

  fireEvent.click(screen.getByRole('link', { name: 'МАРКЕТ' }));
  expect(navigate).toHaveBeenCalledWith('/market');
});
