// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { Footer } from './Footer';

afterEach(cleanup);

it('gives every footer destination a real route and client-side navigation', () => {
  const navigate = vi.fn();
  render(<Footer navigate={navigate} />);

  const destinations = [
    ['Контракты', '/contracts'],
    ['Маркет', '/market'],
    ['Инвентарь', '/inventory'],
    ['История', '/history'],
    ['Прозрачность', '/transparency'],
    ['Поддержка', '/support'],
    ['Условия использования', '/terms'],
    ['Конфиденциальность', '/privacy'],
  ] as const;

  for (const [label, href] of destinations) {
    const link = screen.getByRole('link', { name: label });
    expect(link.getAttribute('href')).toBe(href);
    fireEvent.click(link);
    expect(navigate).toHaveBeenLastCalledWith(href);
  }

  expect(screen.queryByText('18+')).toBeNull();
  expect(screen.queryByText(/не аффилирован/i)).toBeNull();
});
