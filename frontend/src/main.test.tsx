// @vitest-environment jsdom
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

beforeEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
  window.history.replaceState({}, '', '/contracts');
  document.body.innerHTML = '<div id="root"></div>';
  document.title = '';
});

afterEach(async () => {
  const { appRoot } = await import('./main');
  appRoot.unmount();
});

it('distinguishes an unauthenticated 401 from an unavailable API', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
    error: { code: 'UNAUTHORIZED', message: 'Authentication required' },
  }), { status: 401, headers: { 'Content-Type': 'application/json' } })));

  await import('./main');

  await waitFor(() => expect(document.querySelector('.live-copy')?.textContent).toContain('AUTHENTICATION REQUIRED'));
  expect(document.querySelector('.live-copy')?.textContent).not.toContain('API UNAVAILABLE');
  expect(document.querySelector('h1')?.textContent).toBe('Требуется вход');
});

it('uses the session hook for login and logout behavior', async () => {
  window.history.replaceState({}, '', '/login');
  const account = { user_id: 'user-id', login: 'tester', created_at: '2026-09-24T12:00:00Z' };
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const pathname = new URL(String(input)).pathname;
    if (pathname.endsWith('/auth/logout')) return new Response(null, { status: 204 });
    if (pathname.endsWith('/auth/login')) return new Response(JSON.stringify({ user_id: account.user_id }), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (pathname.endsWith('/me')) return new Response(JSON.stringify(account), { status: 200, headers: { 'Content-Type': 'application/json' } });
    return new Response(JSON.stringify({ items: [] }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  });
  vi.stubGlobal('fetch', fetchMock);

  await import('./main');
  fireEvent.change(await screen.findByLabelText('ЛОГИН'), { target: { value: 'tester' } });
  fireEvent.change(screen.getByLabelText('ПАРОЛЬ'), { target: { value: 'secret' } });
  fireEvent.click(screen.getByRole('button', { name: /ПРОДОЛЖИТЬ/ }));
  await waitFor(() => expect(window.location.pathname).toBe('/contracts'));
  fireEvent.click(screen.getByRole('button', { name: 'Открыть меню профиля' }));
  fireEvent.click(await screen.findByRole('menuitem', { name: /Выйти/ }));
  await waitFor(() => expect(window.location.pathname).toBe('/login'));
  expect(fetchMock.mock.calls.some(([url]) => new URL(String(url)).pathname.endsWith('/auth/login'))).toBe(true);
  expect(fetchMock.mock.calls.some(([url]) => new URL(String(url)).pathname.endsWith('/auth/logout'))).toBe(true);
});

it('shows owned login validation instead of sending empty credentials', async () => {
  window.history.replaceState({}, '', '/login');
  const fetchMock = vi.fn(async () => new Response(JSON.stringify({ error: { code: 'UNAUTHORIZED' } }), { status: 401 }));
  vi.stubGlobal('fetch', fetchMock);
  await import('./main');
  fireEvent.click(await screen.findByRole('button', { name: /ПРОДОЛЖИТЬ/ }));
  expect((await screen.findByRole('alert')).textContent).toContain('Введите логин и пароль.');
  expect(fetchMock.mock.calls.some((call) => String((call as unknown as [RequestInfo | URL])[0]).includes('/auth/login'))).toBe(false);
});

it('replaces development fixtures when the live inventory is empty', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ items: [] }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })));

  await import('./main');

  await waitFor(() => {
    expect(document.querySelector('.live-copy')?.textContent).toContain('API CONNECTED');
  });
  expect(document.querySelectorAll('.inventory-grid .skin-card')).toHaveLength(0);
  expect(document.querySelector('.section-head span')?.textContent).toContain('0 / 10');
});

it('does not leave development inventory visible when the API fails without fallback opt-in', async () => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
    error: { code: 'SERVICE_UNAVAILABLE', message: 'Service temporarily unavailable' },
  }), {
    status: 503,
    headers: { 'Content-Type': 'application/json' },
  })));

  await import('./main');

  await waitFor(() => {
    expect(document.querySelector('.live-copy')?.textContent).toContain('API UNAVAILABLE');
  });
  expect(document.querySelectorAll('.inventory-grid .skin-card')).toHaveLength(0);
  expect(document.querySelector('.live-count')?.textContent).toBe('ONLINE: НЕДОСТУПНО');
});

it.each([
  ['/market', 'Маркет'],
  ['/inventory', 'Инвентарь'],
  ['/history', 'История контрактов'],
  ['/contracts/CTR-8F4A91', 'Контракт CTR-8F4A91'],
  ['/profile', 'Профиль'],
  ['/login', 'Войти'],
  ['/transparency', 'Прозрачность'],
  ['/terms', 'Условия использования'],
  ['/privacy', 'Политика конфиденциальности'],
  ['/support', 'Поддержка'],
  ['/missing', 'Страница не найдена'],
])('renders the correct page on direct refresh at %s', async (pathname, heading) => {
  window.history.replaceState({}, '', pathname);

  await import('./main');

  await waitFor(() => {
    expect(document.querySelector('h1')?.textContent).toBe(heading);
  });
});

it('keeps API availability unclaimed on a direct route that makes no request', async () => {
  window.history.replaceState({}, '', '/market');

  await import('./main');

  await waitFor(() => {
    expect(document.querySelector('.live-copy')?.textContent).toContain('API NOT CHECKED');
  });
  expect(document.querySelector('.live-copy')?.textContent).not.toContain('API UNAVAILABLE');
});
