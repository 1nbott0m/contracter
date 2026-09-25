import type { InventoryFilterValues } from '../types';

type InventoryFiltersProps = {
  value: InventoryFilterValues;
  weapons: string[];
  rarities: string[];
  wears: string[];
  onChange: (value: InventoryFilterValues) => void;
};

export const DEFAULT_INVENTORY_FILTERS: InventoryFilterValues = {
  search: '', weapon: '', rarity: '', wear: '', minPrice: '', maxPrice: '', sort: 'newest',
};

export function InventoryFilters({ value, weapons, rarities, wears, onChange }: InventoryFiltersProps) {
  const update = <K extends keyof InventoryFilterValues>(key: K, next: InventoryFilterValues[K]) => {
    onChange({ ...value, [key]: next });
  };

  return <div className="commerce-filters inventory-filters" aria-label="Фильтры инвентаря">
    <label className="commerce-search">Поиск<input type="search" value={value.search} onChange={(event) => update('search', event.target.value)} placeholder="Название скина" /></label>
    <label>Оружие<select value={value.weapon} onChange={(event) => update('weapon', event.target.value)}><option value="">Все</option>{weapons.map((weapon) => <option key={weapon}>{weapon}</option>)}</select></label>
    <label>Редкость<select value={value.rarity} onChange={(event) => update('rarity', event.target.value)}><option value="">Все</option>{rarities.map((rarity) => <option key={rarity}>{rarity}</option>)}</select></label>
    <label>Износ<select value={value.wear} onChange={(event) => update('wear', event.target.value)}><option value="">Любой</option>{wears.map((wear) => <option key={wear}>{wear}</option>)}</select></label>
    <label>Цена от<input type="number" min="0" step="0.01" value={value.minPrice} onChange={(event) => update('minPrice', event.target.value)} /></label>
    <label>Цена до<input type="number" min="0" step="0.01" value={value.maxPrice} onChange={(event) => update('maxPrice', event.target.value)} /></label>
    <label>Сортировка<select value={value.sort} onChange={(event) => update('sort', event.target.value as InventoryFilterValues['sort'])}><option value="newest">Сначала новые</option><option value="name">По названию</option><option value="price-asc">Цена по возрастанию</option><option value="price-desc">Цена по убыванию</option></select></label>
  </div>;
}
