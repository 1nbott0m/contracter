import { useEffect, useState, type MouseEvent, type ReactNode } from 'react';
import { ArrowRight, RefreshCw } from 'lucide-react';
import { api, type ContractHistoryItem, type InventoryEventHistoryItem, type LedgerHistoryItem } from '../api';
import type { ApiStatus } from '../components/AppShell';
import { readContractPresentation } from '../components/ContractReveal';
import { resolveSkinImage, SkinImage } from '../components/SkinImage';
import type { Navigate } from '../router';
import type { SessionState } from '../session';
import type { AsyncState } from '../types';

type HistoryApi = Pick<typeof api, 'history'> & Partial<Pick<typeof api, 'ledgerHistory' | 'inventoryEventHistory'>>;
type HistoryMode = 'contracts' | 'ledger' | 'inventory';
type HistoryPageProps = { session: SessionState; navigate: Navigate; client?: HistoryApi; setApiStatus?: (status: ApiStatus) => void };

export function contractStatusLabel(status: string) {
  const normalized = status.trim().toLowerCase();
  if (['completed', 'accepted', 'settled'].includes(normalized)) return 'Завершён';
  if (['failed', 'rejected', 'cancelled', 'canceled'].includes(normalized)) return 'Не завершён';
  return 'В обработке';
}

export function contractDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? 'Дата недоступна' : new Intl.DateTimeFormat('ru-RU', { dateStyle: 'medium', timeStyle: 'short' }).format(date);
}

export function microcredits(value: number | null) {
  return value === null ? 'Недоступно' : `${(value / 1_000_000).toLocaleString('ru-RU', { maximumFractionDigits: 6 })} CC`;
}

export function HistoryPage({ session, navigate, client = api, setApiStatus }: HistoryPageProps) {
  const [state, setState] = useState<AsyncState<ContractHistoryItem[]>>({ status: 'loading' });
  const [ledgerState, setLedgerState] = useState<AsyncState<LedgerHistoryItem[]>>({ status: 'loading' });
  const [inventoryState, setInventoryState] = useState<AsyncState<InventoryEventHistoryItem[]>>({ status: 'loading' });
  const [mode, setMode] = useState<HistoryMode>('contracts');
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (session.status !== 'authenticated') return;
    let active = true;
    setState({ status: 'loading' });
    setApiStatus?.('connecting');
    const load = mode === 'contracts'
      ? client.history()
      : mode === 'ledger'
        ? client.ledgerHistory?.() ?? Promise.resolve([])
        : client.inventoryEventHistory?.() ?? Promise.resolve([]);
    load.then((items) => {
      if (!active) return;
      if (mode === 'contracts') setState(items.length ? { status: 'success', data: items as ContractHistoryItem[] } : { status: 'empty', data: [] });
      if (mode === 'ledger') setLedgerState(items.length ? { status: 'success', data: items as LedgerHistoryItem[] } : { status: 'empty', data: [] });
      if (mode === 'inventory') setInventoryState(items.length ? { status: 'success', data: items as InventoryEventHistoryItem[] } : { status: 'empty', data: [] });
      setApiStatus?.('live');
    }).catch((error: unknown) => {
      if (!active) return;
      setState({ status: 'error', error: error instanceof Error ? error.message : 'История недоступна' });
      setApiStatus?.('fallback');
    });
    return () => { active = false; };
  }, [client, mode, reload, session.status, setApiStatus]);

  if (session.status !== 'authenticated') {
    return <section className="route-state"><span className="eyebrow">ACCOUNT / PRIVATE HISTORY</span><h1>История контрактов</h1><p>{session.status === 'loading' ? 'Проверяем сессию.' : 'Войдите, чтобы загрузить историю этого аккаунта.'}</p>{session.status !== 'loading' && <button className="primary route-state-action" type="button" onClick={() => navigate('/login?returnTo=/history')}>Войти <ArrowRight size={16} /></button>}</section>;
  }

  const open = (event: MouseEvent<HTMLAnchorElement>, contractId: string) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(`/contracts/${encodeURIComponent(contractId)}`);
  };

  return <>
    <section className="intro commerce-intro"><div><span className="eyebrow">ACCOUNT / PRIVATE HISTORY</span><h1>История операций</h1><p>Только события текущего авторизованного аккаунта.</p></div></section>
    <div className="history-tabs" role="tablist" aria-label="Тип истории">{([['contracts', 'Контракты'], ['ledger', 'Баланс CC'], ['inventory', 'Инвентарь']] as const).map(([value, label]) => <button key={value} type="button" role="tab" aria-selected={mode === value} className={mode === value ? 'active' : ''} onClick={() => setMode(value)}>{label}</button>)}</div>
    {mode === 'contracts' && state.status === 'loading' && <div className="commerce-state" role="status"><RefreshCw className="spin" size={18} /> Загружаем историю…</div>}
    {mode === 'contracts' && state.status === 'error' && <div className="commerce-state" role="alert"><strong>История недоступна</strong><span>Демонстрационные операции не подставлены.</span><button type="button" onClick={() => setReload((value) => value + 1)}>Повторить</button></div>}
    {mode === 'contracts' && state.status === 'empty' && <div className="commerce-state"><strong>История пуста</strong><span>API не вернул контрактов этого аккаунта.</span></div>}
    {mode === 'contracts' && state.status === 'success' && <section className="history-list" aria-label="Контракты аккаунта">{state.data.map((entry) => {
      const presentation = readContractPresentation(entry.contract_id);
      return <article className="history-card panel" key={entry.contract_id}>
        <div className="history-card-heading"><div><span className="eyebrow">{contractStatusLabel(entry.status)}</span><h2>{entry.contract_id}</h2><time dateTime={entry.created_at}>{contractDate(entry.created_at)}</time></div><a href={`/contracts/${encodeURIComponent(entry.contract_id)}`} onClick={(event) => open(event, entry.contract_id)} aria-label={`Открыть контракт ${entry.contract_id}`}>Открыть <ArrowRight size={15} /></a></div>
        {presentation ? <div className="history-contract-visual">
          <div className="history-inputs" aria-label="Входные предметы">{presentation.inputs.map((item, index) => <div className="history-art" key={`${item.id}-${index}`}><SkinImage src={resolveSkinImage(item)} alt={`${item.weapon} | ${item.skin}`} accent={item.color} /></div>)}</div>
          <ArrowRight className="history-direction" aria-hidden="true" />
          {presentation.result ? <div className="history-result"><div className="history-art"><SkinImage src={resolveSkinImage(presentation.result)} alt={`${presentation.result.weapon} | ${presentation.result.skin}`} accent={presentation.result.color} /></div><strong>{presentation.result.weapon} | {presentation.result.skin}</strong></div> : <div className="history-result-unavailable">Artwork результата недоступен</div>}
          <dl className="history-values"><div><dt>Входы</dt><dd>{microcredits(presentation.inputValueMicrocredits)}</dd></div><div><dt>Результат</dt><dd>{microcredits(presentation.resultValueMicrocredits)}</dd></div></dl>
        </div> : <p className="history-media-unavailable">Изображения и значения недоступны в текущем API истории.</p>}
      </article>;
    })}</section>}
    {mode === 'ledger' && <HistoryRows state={ledgerState} empty="Операций CC пока нет" title="История баланса CC" render={(entry) => <><strong>{entry.operation}</strong><span>{microcredits(entry.amount_microcredits)}</span><time dateTime={entry.occurred_at}>{contractDate(entry.occurred_at)}</time></>} />}
    {mode === 'inventory' && <HistoryRows state={inventoryState} empty="Событий инвентаря пока нет" title="История инвентаря" render={(entry) => <><strong>{entry.event_kind}</strong><span>Предмет {entry.inventory_item_id}</span><time dateTime={entry.occurred_at}>{contractDate(entry.occurred_at)}</time></>} />}
  </>;
}

function HistoryRows<T>({ state, empty, title, render }: { state: AsyncState<T[]>; empty: string; title: string; render: (entry: T) => ReactNode }) {
  if (state.status === 'loading') return <div className="commerce-state" role="status"><RefreshCw className="spin" size={18} /> Загружаем…</div>;
  if (state.status === 'error') return <div className="commerce-state" role="alert"><strong>История недоступна</strong><span>Демонстрационные события не подставлены.</span></div>;
  if (state.status === 'empty' || state.status === 'idle') return <div className="commerce-state"><strong>{empty}</strong></div>;
  return <section className="history-list" aria-label={title}>{state.data.map((entry, index) => <article className="history-card panel history-row" key={index}>{render(entry)}</article>)}</section>;
}
