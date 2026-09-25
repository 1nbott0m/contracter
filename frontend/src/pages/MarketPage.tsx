import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowRight, RefreshCw, ShoppingCart, X } from 'lucide-react';
import { api, catalogSkuToMarketItem } from '../api';
import type { ApiStatus } from '../components/AppShell';
import { resolveSkinImage, SkinImage } from '../components/SkinImage';
import { money } from '../components/SkinCard';
import type { Navigate } from '../router';
import type { SessionState } from '../session';
import type { AsyncState, MarketItem } from '../types';

type MarketApi = Pick<typeof api, 'catalogSkus' | 'marketValuations' | 'balance' | 'inventory' | 'marketPurchase'>;
type PurchaseState = { status: 'idle' | 'loading' | 'success' | 'error'; message?: string };
type MarketSort = 'name' | 'price-asc' | 'price-desc';

type MarketPageProps = {
  session: SessionState;
  navigate: Navigate;
  client?: MarketApi;
  onBalanceChange?: (balanceMicrocredits: number | null) => void;
  setApiStatus?: (status: ApiStatus) => void;
};

function currentSearch() {
  return typeof window === 'undefined' ? '' : new URLSearchParams(window.location.search).get('search') || '';
}

export function MarketPage({ session, navigate, client = api, onBalanceChange, setApiStatus }: MarketPageProps) {
  const [state, setState] = useState<AsyncState<MarketItem[]>>({ status: 'loading' });
  const [search, setSearch] = useState(currentSearch);
  const [rarity, setRarity] = useState('');
  const [wear, setWear] = useState('');
  const [availability, setAvailability] = useState('all');
  const [sort, setSort] = useState<MarketSort>('name');
  const [balance, setBalance] = useState<number | null>(null);
  const [purchases, setPurchases] = useState<Record<string, PurchaseState>>({});
  const [retryAttempt, setRetryAttempt] = useState(0);
  const purchaseKeys = useRef(new Map<string, string>());

  useEffect(() => {
    let active = true;
    setState({ status: 'loading' });
    setApiStatus?.('connecting');
    Promise.all([client.catalogSkus(), client.marketValuations()])
      .then(([catalog, valuations]) => {
        if (!active) return;
        const bySku = new Map(valuations.items.map((valuation) => [valuation.sku_id, valuation]));
        const items = catalog.items.map((sku) => catalogSkuToMarketItem(sku, bySku.get(sku.sku_id)));
        setState(items.length ? { status: 'success', data: items } : { status: 'empty', data: [] });
        setApiStatus?.('live');
      })
      .catch((error: unknown) => {
        if (!active) return;
        setState({ status: 'error', error: error instanceof Error ? error.message : 'Маркет недоступен' });
        setApiStatus?.('fallback');
      });
    if (session.status === 'authenticated') {
      client.balance().then((value) => {
        if (!active) return;
        setBalance(value.balance_microcredits);
        onBalanceChange?.(value.balance_microcredits);
      }).catch(() => {
        if (!active) return;
        setBalance(null);
        onBalanceChange?.(null);
      });
    }
    return () => { active = false; };
  // Loading is scoped to the session and an explicit retry. The injected API
  // wrapper may be recreated by an embedding surface without changing data.
  }, [retryAttempt, session.status]);

  const source = state.status === 'success' || state.status === 'empty' ? state.data || [] : [];
  const hasPublishedPrices = source.some((item) => item.price !== null);
  const rarities = useMemo(() => [...new Set(source.map((item) => item.rarity))].sort(), [source]);
  const wears = useMemo(() => [...new Set(source.map((item) => item.wear))].sort(), [source]);
  const filtered = useMemo(() => {
    const query = search.trim().toLocaleLowerCase('ru');
    return source
      .filter((item) => !query || `${item.weapon} ${item.skin}`.toLocaleLowerCase('ru').includes(query))
      .filter((item) => !rarity || item.rarity === rarity)
      .filter((item) => !wear || item.wear === wear)
      .filter((item) => availability === 'all' || (availability === 'available' ? item.available : !item.available))
      .sort((left, right) => {
        if (sort === 'name') return `${left.weapon} ${left.skin}`.localeCompare(`${right.weapon} ${right.skin}`, 'ru');
        const leftPrice = left.price ?? Number.POSITIVE_INFINITY;
        const rightPrice = right.price ?? Number.POSITIVE_INFINITY;
        return sort === 'price-asc' ? leftPrice - rightPrice : rightPrice - leftPrice;
      });
  }, [availability, rarity, search, sort, source, wear]);

  const retryLoad = () => setRetryAttempt((attempt) => attempt + 1);
  const purchase = async (item: MarketItem) => {
    if (session.status !== 'authenticated') {
      navigate('/login');
      return;
    }
    if (!item.available || item.price === null || purchases[item.skuId]?.status === 'loading') return;
    const key = purchaseKeys.current.get(item.skuId) || crypto.randomUUID();
    purchaseKeys.current.set(item.skuId, key);
    setPurchases((current) => ({ ...current, [item.skuId]: { status: 'loading' } }));
    try {
      await client.marketPurchase(item.skuId, key);
      purchaseKeys.current.delete(item.skuId);
      const [nextBalance, nextInventory] = await Promise.allSettled([client.balance(), client.inventory()]);
      if (nextBalance.status === 'fulfilled') {
        setBalance(nextBalance.value.balance_microcredits);
        onBalanceChange?.(nextBalance.value.balance_microcredits);
      }
      const refreshed = nextBalance.status === 'fulfilled' && nextInventory.status === 'fulfilled';
      setPurchases((current) => ({ ...current, [item.skuId]: {
        status: 'success',
        message: refreshed ? 'Покупка подтверждена. Баланс и инвентарь обновлены.' : 'Покупка подтверждена, но обновить баланс или инвентарь не удалось.',
      } }));
    } catch {
      setPurchases((current) => ({ ...current, [item.skuId]: { status: 'error', message: 'Не удалось подтвердить покупку. Повтор использует тот же ключ операции.' } }));
    }
  };

  return <>
    <section className="intro commerce-intro"><div><span className="eyebrow">MARKET / CC ECONOMY</span><h1>Маркет</h1><p>Публичный каталог с текущими подтверждёнными сервером оценками.</p></div><div className="commerce-balance">БАЛАНС: {session.status === 'authenticated' && balance !== null ? `${(balance / 1_000_000).toLocaleString('ru-RU')} CC` : 'НЕДОСТУПЕН'}</div></section>
    <div className="commerce-filters" aria-label="Фильтры маркета">
      <label className="commerce-search">Поиск<span className="commerce-search-field"><input aria-label="Поиск по маркету" type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Оружие или скин" />{search && <button type="button" className="commerce-search-clear" aria-label="Очистить поиск маркета" onClick={() => setSearch('')}><X size={14} /></button>}</span></label>
      {/* Native selects are intentional: compact market filters use the OS picker. */}<label>Редкость<select value={rarity} onChange={(event) => setRarity(event.target.value)}><option value="">Все</option>{rarities.map((value) => <option key={value}>{value}</option>)}</select></label>
      <label>Износ<select value={wear} onChange={(event) => setWear(event.target.value)}><option value="">Любой</option>{wears.map((value) => <option key={value}>{value}</option>)}</select></label>
      <label>Наличие<select value={availability} onChange={(event) => setAvailability(event.target.value)}><option value="all">Все</option><option value="available">Доступно</option><option value="unavailable">Недоступно</option></select></label>
      <label>Сортировка<select value={sort} onChange={(event) => setSort(event.target.value as MarketSort)}><option value="name">По названию</option><option value="price-asc">Цена по возрастанию</option><option value="price-desc">Цена по убыванию</option></select></label>
    </div>
    {state.status === 'loading' && <div className="commerce-state" role="status"><RefreshCw className="spin" size={18} /> Загружаем маркет…</div>}
    {state.status === 'error' && <div className="commerce-state" role="alert"><strong>Маркет недоступен</strong><span>Данные не заменены демонстрационными значениями.</span><button type="button" onClick={retryLoad}>Повторить</button></div>}
    {state.status === 'empty' && <div className="commerce-state"><strong>Предложений нет</strong><span>API вернул пустой каталог.</span></div>}
    {source.length > 0 && !hasPublishedPrices && <div className="commerce-state market-pricing-notice" role="status"><strong>Цены CC ещё не опубликованы</strong><span>Каталог уже загружен с сервера. Покупки включатся после публикации подтверждённых оценок.</span></div>}
    {(state.status === 'success' || state.status === 'empty') && filtered.length === 0 && source.length > 0 && <div className="commerce-state"><strong>Ничего не найдено</strong><span>Измените поисковый запрос или фильтры.</span></div>}
    {filtered.length > 0 && <section className="market-grid" aria-label="Предложения маркета">{filtered.map((item) => {
      const purchaseState = purchases[item.skuId] || { status: 'idle' as const };
      const name = `${item.weapon} | ${item.skin}`;
      const unavailable = !item.available || item.price === null;
      const loading = purchaseState.status === 'loading';
      return <article className="market-card panel" key={item.skuId}>
        <div className="market-art"><SkinImage src={resolveSkinImage(item)} alt={name} accent={item.color} /></div>
        <span className="eyebrow">{item.rarity}</span><h2>{item.weapon}<strong>{item.skin}</strong></h2><p>{item.wear}</p>
        <div className="market-price"><strong>{money(item.price)}</strong><span>{item.available ? 'ДОСТУПНО' : 'НЕДОСТУПНО'}</span></div>
        <button className="primary market-buy" type="button" disabled={unavailable || loading || purchaseState.status === 'success'} onClick={() => void purchase(item)} aria-label={unavailable ? `${name} недоступен для покупки` : loading ? `Покупка ${name} выполняется` : purchaseState.status === 'success' ? `${name} куплен` : `Купить ${name} за ${money(item.price)}`}>
          {loading ? <><RefreshCw className="spin" size={15} /> ПОКУПКА…</> : purchaseState.status === 'success' ? <>КУПЛЕНО <ArrowRight size={15} /></> : <><ShoppingCart size={15} /> {unavailable ? 'НЕДОСТУПНО' : 'КУПИТЬ'}</>}
        </button>
        {purchaseState.message && <p className={`purchase-message ${purchaseState.status}`} role={purchaseState.status === 'error' ? 'alert' : 'status'}>{purchaseState.message}</p>}
      </article>;
    })}</section>}
  </>;
}
