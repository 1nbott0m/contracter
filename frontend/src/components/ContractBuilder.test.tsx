// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { InventoryItem } from '../types';
import { ContractBuilder } from './ContractBuilder';

const items: InventoryItem[] = Array.from({ length: 11 }, (_, index) => ({
  id: `item-${index + 1}`,
  publicId: `public-item-${index + 1}`,
  weapon: `Weapon ${index + 1}`,
  skin: `Skin ${index + 1}`,
  wear: 'Minimal Wear',
  price: index + 1,
  color: '#58d6e7',
  rarity: 'restricted',
  image: `https://cdn.example.test/item-${index + 1}.png`,
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

afterEach(cleanup);

describe('ContractBuilder selection', () => {
  it('adds and removes items while rendering ten total contract slots', () => {
    render(<ContractBuilder items={items} initialSelected={items.slice(0, 4)} onSubmit={vi.fn()} />);

    expect(screen.getByText('4 / 10 ПРЕДМЕТОВ')).toBeTruthy();
    expect(screen.getAllByTestId('contract-slot')).toHaveLength(10);

    fireEvent.click(screen.getByRole('button', { name: 'Выбрать Weapon 5 | Skin 5 для контракта' }));
    expect(screen.getByText('5 / 10 ПРЕДМЕТОВ')).toBeTruthy();

    fireEvent.click(screen.getAllByRole('button', { name: 'Убрать Weapon 1 | Skin 1 из контракта' })[0]!);
    expect(screen.getByText('4 / 10 ПРЕДМЕТОВ')).toBeTruthy();
  });

  it('never selects more than ten distinct unlocked items', () => {
    render(<ContractBuilder items={items} initialSelected={items.slice(0, 9)} onSubmit={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Выбрать Weapon 10 | Skin 10 для контракта' }));
    expect(screen.getByText('10 / 10 ПРЕДМЕТОВ')).toBeTruthy();

    const eleventh = screen.getByRole('button', { name: 'Выбрать Weapon 11 | Skin 11 для контракта' });
    expect((eleventh as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(eleventh);
    expect(screen.getByText('10 / 10 ПРЕДМЕТОВ')).toBeTruthy();
  });

  it('lets the user inspect an item without changing the selection', () => {
    render(<ContractBuilder items={items} initialSelected={items.slice(0, 4)} onSubmit={vi.fn()} />);

    fireEvent.click(screen.getAllByRole('button', { name: 'Подробнее о Weapon 1 | Skin 1' })[0]!);

    const dialog = screen.getByRole('dialog', { name: 'Weapon 1 | Skin 1' });
    expect(dialog).toBeTruthy();
    expect(within(dialog).getByText('Minimal Wear')).toBeTruthy();
    expect(screen.getByText('4 / 10 ПРЕДМЕТОВ')).toBeTruthy();
  });
});

describe('ContractBuilder submission', () => {
  it('keeps the CTA disabled below four items and submits the exact selected public ids when ready', async () => {
    const onSubmit = vi.fn(async () => ({ contractId: 'server-contract-id' }));
    render(<ContractBuilder items={items} initialSelected={items.slice(0, 3)} onSubmit={onSubmit} />);

    expect((screen.getByRole('button', { name: 'ВЫБЕРИТЕ ЕЩЁ 1' }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Выбрать Weapon 4 | Skin 4 для контракта' }));
    fireEvent.click(screen.getByRole('button', { name: 'ЗАКЛЮЧИТЬ КОНТРАКТ' }));

    expect(screen.getByRole('button', { name: 'ФИКСИРУЕМ…' })).toBeTruthy();
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(items.slice(0, 4)));
    expect(await screen.findByText('Контракт server-contract-id принят сервером.')).toBeTruthy();
  });

  it('renders a retryable error state without inventing a committed result', async () => {
    const onSubmit = vi.fn(async () => { throw new Error('backend unavailable'); });
    render(<ContractBuilder items={items} initialSelected={items.slice(0, 4)} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByRole('button', { name: 'ЗАКЛЮЧИТЬ КОНТРАКТ' }));

    expect((await screen.findByRole('alert')).textContent).toContain('Не удалось заключить контракт');
    expect(screen.getByRole('button', { name: 'ПОВТОРИТЬ' })).toBeTruthy();
    expect(screen.queryByText(/ВАШ РЕЗУЛЬТАТ/)).toBeNull();
  });

  it('freezes the immutable submitted snapshot while the request is pending', async () => {
    const pending = deferred<{ contractId: string }>();
    const onSubmit = vi.fn((_submitted: readonly InventoryItem[]) => pending.promise);
    render(<ContractBuilder items={items} initialSelected={items.slice(0, 4)} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByRole('button', { name: 'ЗАКЛЮЧИТЬ КОНТРАКТ' }));

    const submitted = onSubmit.mock.calls[0]?.[0];
    expect(Object.isFrozen(submitted)).toBe(true);
    expect(submitted).toEqual(items.slice(0, 4));

    const remove = screen.getAllByRole('button', { name: 'Убрать Weapon 1 | Skin 1 из контракта' })[0] as HTMLButtonElement;
    const add = screen.getByRole('button', { name: 'Выбрать Weapon 5 | Skin 5 для контракта' }) as HTMLButtonElement;
    expect(remove.disabled).toBe(true);
    expect(add.disabled).toBe(true);
    fireEvent.click(remove);
    fireEvent.click(add);
    expect(screen.getByText('4 / 10 ПРЕДМЕТОВ')).toBeTruthy();
    expect(submitted).toEqual(items.slice(0, 4));

    pending.resolve({ contractId: 'server-contract-id' });
    expect(await screen.findByText('Контракт server-contract-id принят сервером.')).toBeTruthy();
  });
});
