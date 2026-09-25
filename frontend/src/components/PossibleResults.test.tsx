// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { PossibleResults } from './PossibleResults';

afterEach(cleanup);

it('renders only candidates with canonical artwork and never invents probabilities', () => {
  render(<PossibleResults items={[
    { id: 'real', weapon: 'AK-47', skin: 'Slate', wear: 'Field-Tested', price: null, color: '#58d6e7', rarity: 'restricted', image: 'https://cdn.example.test/real.png' },
    { id: 'missing', weapon: 'M4A1-S', skin: 'Missing', wear: 'Field-Tested', price: null, color: '#58d6e7', rarity: 'restricted', image: '' },
  ]} />);

  expect(screen.getByRole('img', { name: 'AK-47 | Slate' }).getAttribute('src')).toBe('https://cdn.example.test/real.png');
  expect(screen.queryByText('Missing')).toBeNull();
  expect(document.body.textContent).not.toMatch(/\d+(?:[.,]\d+)?\s*%/);
  expect(screen.getByRole('heading', { name: 'НЕПРОВЕРЕННЫЙ ПРЕДПРОСМОТР КАТАЛОГА' })).toBeTruthy();
  expect(document.body.textContent).toContain('не подтверждает допустимость результата');
  expect(document.body.textContent).not.toContain('ВОЗМОЖНЫЕ РЕЗУЛЬТАТЫ');
});
