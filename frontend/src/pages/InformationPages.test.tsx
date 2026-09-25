// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { PrivacyPage } from './PrivacyPage';
import { SupportPage } from './SupportPage';
import { TermsPage } from './TermsPage';
import { TransparencyPage } from './TransparencyPage';

afterEach(cleanup);

it('states the real private-history verification boundary on transparency', () => {
  render(<TransparencyPage session={{ status: 'unauthenticated' }} navigate={vi.fn()} />);
  expect(screen.getByRole('heading', { name: 'Прозрачность' })).toBeTruthy();
  expect(screen.getByText(/не является публичной криптографической проверкой/i)).toBeTruthy();
  expect(screen.getByText(/4–10/)).toBeTruthy();
});

it('publishes honest legal and support boundaries without fake contacts or policies', () => {
  const { rerender } = render(<TermsPage />);
  expect(screen.getByRole('heading', { name: 'Условия использования' })).toBeTruthy();
  expect(screen.getByText(/CC — внутренняя учётная единица/i)).toBeTruthy();

  rerender(<PrivacyPage />);
  expect(screen.getByRole('heading', { name: 'Политика конфиденциальности' })).toBeTruthy();
  expect(screen.getByText(/опубликованная политика сроков хранения/i)).toBeTruthy();

  rerender(<SupportPage navigate={vi.fn()} />);
  expect(screen.getByRole('heading', { name: 'Поддержка' })).toBeTruthy();
  expect(screen.getByText(/публичный канал поддержки пока не настроен/i)).toBeTruthy();
  expect(screen.queryByRole('link', { name: /@|email/i })).toBeNull();
});
