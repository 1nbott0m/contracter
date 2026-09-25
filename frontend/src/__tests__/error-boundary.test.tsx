// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ErrorBoundary } from '../error-boundary';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('replaces a crashed view with an accessible recovery action', () => {
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
  let shouldThrow = true;

  function UnstableView() {
    if (shouldThrow) throw new Error('render failed');
    return <h1>Recovered view</h1>;
  }

  render(<ErrorBoundary><UnstableView /></ErrorBoundary>);

  expect(screen.getByRole('alert').textContent).toContain('Интерфейс не загрузился');
  shouldThrow = false;
  fireEvent.click(screen.getByRole('button', { name: 'Повторить' }));
  expect(screen.getByRole('heading', { name: 'Recovered view' })).toBeTruthy();
});
