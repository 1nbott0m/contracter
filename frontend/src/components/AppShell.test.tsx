// @vitest-environment jsdom
import { cleanup, createEvent, fireEvent, render, screen, waitFor } from '@testing-library/react';
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

it('does not report the API as unavailable before a request fails', () => {
  render(
    <AppShell route={matchRoute('/market')} navigate={vi.fn()} apiStatus="unverified">
      <h1>Маркет</h1>
    </AppShell>,
  );

  expect(screen.getByText(/API NOT CHECKED/)).toBeTruthy();
  expect(screen.queryByText(/API UNAVAILABLE/)).toBeNull();
});

it('routes the logo in-app while preserving modified-click browser behavior', () => {
  const navigate = vi.fn();
  render(
    <AppShell route={matchRoute('/market')} navigate={navigate} apiStatus="unverified">
      <h1>Маркет</h1>
    </AppShell>,
  );

  const logo = screen.getByRole('link', { name: 'CONTRACTER — Контракты' });
  const ordinaryClick = createEvent.click(logo, { button: 0 });
  fireEvent(logo, ordinaryClick);
  expect(ordinaryClick.defaultPrevented).toBe(true);
  expect(navigate).toHaveBeenCalledWith('/contracts');

  navigate.mockClear();
  let modifiedClickWasPrevented = true;
  window.addEventListener('click', (event) => {
    modifiedClickWasPrevented = event.defaultPrevented;
    event.preventDefault();
  }, { once: true });
  const modifiedClick = createEvent.click(logo, { button: 0, ctrlKey: true });
  fireEvent(logo, modifiedClick);
  expect(modifiedClickWasPrevented).toBe(false);
  expect(navigate).not.toHaveBeenCalled();
});

it('updates the route title and moves focus to the page heading after navigation', async () => {
  const { rerender } = render(
    <AppShell route={matchRoute('/market')} navigate={vi.fn()} apiStatus="unverified">
      <h1>Маркет</h1>
    </AppShell>,
  );

  await waitFor(() => {
    expect(document.title).toBe('Маркет · CONTRACTER');
    expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Маркет' }));
  });

  rerender(
    <AppShell route={matchRoute('/privacy')} navigate={vi.fn()} apiStatus="unverified">
      <h1>Политика конфиденциальности</h1>
    </AppShell>,
  );

  await waitFor(() => {
    expect(document.title).toBe('Политика конфиденциальности · CONTRACTER');
    expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Политика конфиденциальности' }));
  });
});
