// @vitest-environment jsdom
import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { NotFoundPage } from './NotFoundPage';

it('offers a working route back to contracts', () => {
  const navigate = vi.fn();
  render(<NotFoundPage navigate={navigate} />);

  expect(screen.getByRole('heading', { name: /страница не найдена/i })).toBeTruthy();
  fireEvent.click(screen.getByRole('link', { name: /к контрактам/i }));
  expect(navigate).toHaveBeenCalledWith('/contracts');
});
