// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SkinCard } from './SkinCard';

const item = {
  id: 'owned-item',
  weapon: 'AK-47',
  skin: 'Slate',
  wear: 'Minimal Wear',
  price: 1.25,
  color: '#58d6e7',
  rarity: 'Restricted',
  image: '',
};

afterEach(cleanup);

describe('SkinCard selection action', () => {
  it('announces that an already selected item will be removed', () => {
    render(<SkinCard item={item} selected onAdd={() => undefined} />);

    const action = screen.getByRole('button', { name: 'Убрать AK-47 | Slate из контракта' });
    expect(action.getAttribute('aria-pressed')).toBe('true');
  });

  it('does not allow a locked owned item to be selected', () => {
    const onAdd = vi.fn();
    render(<SkinCard item={{ ...item, locked: true }} onAdd={onAdd} />);

    const action = screen.getByRole('button', { name: 'AK-47 | Slate недоступен: предмет заблокирован' });
    expect((action as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(action);
    expect(onAdd).not.toHaveBeenCalled();
  });

  it('shows an unavailable value instead of a zero price when valuation data is missing', () => {
    render(<SkinCard item={{ ...item, price: null }} />);

    expect(screen.getByText('ЦЕНА НЕДОСТУПНА')).toBeTruthy();
    expect(screen.queryByText('0 CC')).toBeNull();
  });
});
