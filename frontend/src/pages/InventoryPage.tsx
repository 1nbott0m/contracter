import { useEffect, useMemo, useState } from 'react';
import { ArrowRight, CirclePlus, RefreshCw } from 'lucide-react';
import { api, inventoryItemToSkin } from '../api';
import type { ApiStatus } from '../components/AppShell';
import { DEFAULT_INVENTORY_FILTERS, InventoryFilters } from '../components/InventoryFilters';
import { SkinCard } from '../components/SkinCard';
import type { Navigate } from '../router';
import type { SessionState } from '../session';
import type { AsyncState, InventoryFilterValues, InventoryItem } from '../types';

export const CONTRACT_SELECTION_STORAGE_KEY = 'contracter.contract.selection';

export function readContractSelectionIds(): string[] {
  try {
    const parsed: unknown = JSON.parse(sessionStorage.getItem(CONTRACT_SELECTION_STORAGE_KEY) || '[]');
    return Array.isArray(parsed) ? parsed.filter((value): value is string => typeof value === 'string').slice(0, 10) : [];
  } catch {
    return [];
  }
}

export function writeContractSelectionIds(ids: string[]) {
  sessionStorage.setItem(CONTRACT_SELECTION_STORAGE_KEY, JSON.stringify([...new Set(ids)].slice(0, 10)));
}

type InventoryApi = Pick<typeof api, 'inventory' | 'catalogSkus' | 'marketValuations'> & Partial<Pick<typeof api, 'inventoryPage'>>;
type InventoryPageProps = { session: SessionState; navigate: Navigate; client?: InventoryApi; setApiStatus?: (status: ApiStatus) => void };

export function InventoryPage({ session, navigate, client = api, setApiStatus }: InventoryPageProps) {
  const [state, setState] = useState<AsyncState<InventoryItem[]>>({ status: 'loading' });
  const [filters, setFilters] = useState<InventoryFilterValues>(DEFAULT_INVENTORY_FILTERS);
  const [selectedIds, setSelectedIds] = useState(readContractSelectionIds);
  const [lastAdded, setLastAdded] = useState<InventoryItem | null>(null);
  const [retryAttempt, setRetryAttempt] = useState(0);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);

  useEffect(() => {
    if (session.status !== 'authenticated') return;
    let active = true;
    setState({ status: 'loading' });
    setApiStatus?.('connecting');
    Promise.all([client.inventoryPage ? client.inventoryPage() : client.inventory(), client.catalogSkus(), client.marketValuations()])
      .then(([inventory, catalog, valuations]) => {
        if (!active) return;
        const catalogBySku = new Map(catalog.items.map((item) => [item.sku_id, item]));
        const valuationBySku = new Map(valuations.items.map((item) => [item.sku_id, item]));
        const items = inventory.items.map((item) => inventoryItemToSkin(item, catalogBySku.get(item.sku_id), valuationBySku.get(item.sku_id)));
        setState(items.length ? { status: 'success', data: items } : { status: 'empty', data: [] });
        setNextCursor('next_cursor' in inventory ? inventory.next_cursor ?? null : null);
        setApiStatus?.('live');
      })
      .catch((error: unknown) => {
        if (!active) return;
        setState({ status: 'error', error: error instanceof Error ? error.message : 'Инвентарь недоступен' });
        setApiStatus?.('fallback');
      });
    return () => { active = false; };
  // Loading is scoped to the session and an explicit retry, not to a wrapper
  // object recreated by a parent render.
  }, [retryAttempt, session.status]);

  const loadMore = async () => {
    if (!nextCursor || !client.inventoryPage || loadingMore) return;
    setLoadingMore(true);
    try {
      const [inventory, catalog, valuations] = await Promise.all([
        client.inventoryPage(nextCursor),
        client.catalogSkus(),
        client.marketValuations(),
      ]);
      const catalogBySku = new Map(catalog.items.map((item) => [item.sku_id, item]));
      const valuationBySku = new Map(valuations.items.map((item) => [item.sku_id, item]));
      const nextItems = inventory.items.map((item) => inventoryItemToSkin(item, catalogBySku.get(item.sku_id), valuationBySku.get(item.sku_id)));
      setState((current) => {
        const existing = current.status === 'success' || current.status === 'empty' ? current.data || [] : [];
        const merged = [...existing, ...nextItems].filter((item, index, all) => all.findIndex((candidate) => candidate.id === item.id) === index);
        return merged.length ? { status: 'success', data: merged } : { status: 'empty', data: [] };
      });
      setNextCursor(inventory.next_cursor ?? null);
    } catch (error: unknown) {
      setState((current) => current.status === 'success' ? current : { status: 'error', error: error instanceof Error ? error.message : 'Инвентарь недоступен' });
    } finally {
      setLoadingMore(false);
    }
  };

  const source = state.status === 'success' || state.status === 'empty' ? state.data || [] : [];
  const weapons = useMemo(() => [...new Set(source.map((item) => item.weapon))].sort(), [source]);
  const rarities = useMemo(() => [...new Set(source.map((item) => item.rarity))].sort(), [source]);
  const wears = useMemo(() => [...new Set(source.map((item) => item.wear))].sort(), [source]);
  const filtered = useMemo(() => {
    const query = filters.search.trim().toLocaleLowerCase('ru');
    const minPrice = filters.minPrice === '' ? null : Number(filters.minPrice);
    const maxPrice = filters.maxPrice === '' ? null : Number(filters.maxPrice);
    return source
      .filter((item) => !query || `${item.weapon} ${item.skin}`.toLocaleLowerCase('ru').includes(query))
      .filter((item) => !filters.weapon || item.weapon === filters.weapon)
      .filter((item) => !filters.rarity || item.rarity === filters.rarity)
      .filter((item) => !filters.wear || item.wear === filters.wear)
      .filter((item) => minPrice === null || (item.price !== null && item.price >= minPrice))
      .filter((item) => maxPrice === null || (item.price !== null && item.price <= maxPrice))
      .sort((left, right) => {
        if (filters.sort === 'name') return `${left.weapon} ${left.skin}`.localeCompare(`${right.weapon} ${right.skin}`, 'ru');
        if (filters.sort === 'newest') return (right.acquiredAt || '').localeCompare(left.acquiredAt || '');
        const leftPrice = left.price ?? Number.POSITIVE_INFINITY;
        const rightPrice = right.price ?? Number.POSITIVE_INFINITY;
        return filters.sort === 'price-asc' ? leftPrice - rightPrice : rightPrice - leftPrice;
      });
  }, [filters, source]);

  if (session.status !== 'authenticated') {
    return <section className="route-state"><span className="eyebrow">COLLECTION / INVENTORY</span><h1>Инвентарь</h1><p>{session.status === 'loading' ? 'Проверяем сессию.' : session.status === 'expired' ? 'Сессия истекла. Войдите снова.' : session.status === 'error' ? 'Не удалось проверить сессию.' : 'Войдите, чтобы загрузить принадлежащие вам предметы.'}</p>{session.status !== 'loading' && <button className="primary route-state-action" type="button" onClick={() => navigate('/login?returnTo=/inventory')}>Войти <ArrowRight size={16} /></button>}</section>;
  }

  const addToContract = (item: InventoryItem) => {
    if (item.locked || selectedIds.includes(item.id) || selectedIds.length >= 10) return;
    const next = [...selectedIds, item.id];
    setSelectedIds(next);
    writeContractSelectionIds(next);
    setLastAdded(item);
  };

  return <>
    <section className="intro commerce-intro"><div><span className="eyebrow">COLLECTION / INVENTORY</span><h1>ИНВЕНТАРЬ</h1><p>Серверные предметы аккаунта. Заблокированные предметы нельзя добавить в новый контракт.</p></div><button className="text-button" type="button" onClick={() => navigate('/contracts')}>К контракту ({selectedIds.length}) <ArrowRight size={15} /></button></section>
    <InventoryFilters value={filters} weapons={weapons} rarities={rarities} wears={wears} onChange={setFilters} />
    {lastAdded && <p className="inventory-add-status" role="status">{lastAdded.weapon} | {lastAdded.skin} добавлен в контракт.</p>}
    {state.status === 'loading' && <div className="commerce-state" role="status"><RefreshCw className="spin" size={18} /> Загружаем инвентарь…</div>}
    {state.status === 'error' && <div className="commerce-state" role="alert"><strong>Инвентарь недоступен</strong><span>Предметы не заменены демонстрационными значениями.</span><button type="button" onClick={() => setRetryAttempt((attempt) => attempt + 1)}>Повторить</button></div>}
    {state.status === 'empty' && <div className="commerce-state"><strong>Инвентарь пуст</strong><span>API не вернул принадлежащих аккаунту предметов.</span></div>}
    {(state.status === 'success' || state.status === 'empty') && source.length > 0 && filtered.length === 0 && <div className="commerce-state"><strong>Ничего не найдено</strong><span>Измените фильтры.</span></div>}
    {filtered.length > 0 && <section className="inventory-page-grid" aria-label="Предметы инвентаря">{filtered.map((item) => {
      const selected = selectedIds.includes(item.id);
      return <div className={`inventory-page-item ${lastAdded?.id === item.id ? 'inventory-item-added' : ''}`} key={item.id}>
        <SkinCard item={item} selected={selected} onAdd={item.locked ? () => undefined : undefined} />
        {!item.locked && <button className="inventory-contract-action" type="button" onClick={() => addToContract(item)} disabled={selected || selectedIds.length >= 10} aria-label={selected ? `${item.weapon} | ${item.skin} уже добавлен в контракт` : `Добавить ${item.weapon} | ${item.skin} в контракт`}><CirclePlus size={15} /> {selected ? 'ДОБАВЛЕНО' : 'В КОНТРАКТ'}</button>}
      </div>;
    })}</section>}
    {nextCursor && <div className="commerce-load-more"><button type="button" className="secondary" onClick={() => void loadMore()} disabled={loadingMore}>{loadingMore ? 'ЗАГРУЗКА…' : 'ЗАГРУЗИТЬ ЕЩЁ'}</button></div>}
  </>;
}
