// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { Logo } from './Logo';

afterEach(cleanup);

it('renders the repository-owned two-card logo asset and routes home', () => {
  const navigate = vi.fn();
  render(<Logo navigate={navigate} />);

  const link = screen.getByRole('link', { name: 'CONTRACTER — Контракты' });
  expect(link.getAttribute('href')).toBe('/contracts');
  expect(link.querySelector('img')?.getAttribute('src')).toBe('/contracter-logo.svg');
  expect(link.querySelector('img')?.getAttribute('alt')).toBe('');

  fireEvent.click(link);
  expect(navigate).toHaveBeenCalledWith('/contracts');
});
