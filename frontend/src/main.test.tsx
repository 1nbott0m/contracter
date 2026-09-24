// @vitest-environment jsdom
import { waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

beforeEach(() => {
  vi.resetModules();
  window.history.replaceState({}, '', '/contracts');
  document.body.innerHTML = '<div id="root"></div>';
  document.title = '';
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
