// @vitest-environment jsdom
import { waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';

beforeEach(() => {
  vi.resetModules();
  document.body.innerHTML = '<div id="root"></div>';
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
