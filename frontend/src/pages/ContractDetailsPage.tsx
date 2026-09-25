import { useEffect, useState } from 'react';
import { ArrowLeft, ArrowRight, RefreshCw } from 'lucide-react';
import { api, type ContractHistoryItem } from '../api';
import { readContractPresentation } from '../components/ContractReveal';
import { resolveSkinImage, SkinImage } from '../components/SkinImage';
import type { Navigate } from '../router';
import type { SessionState } from '../session';
import type { AsyncState } from '../types';
import { contractDate, contractStatusLabel, microcredits } from './HistoryPage';

type ContractDetailsApi = Pick<typeof api, 'findMyContractHistoryEntry'>;
type ContractDetailsPageProps = { contractId: string; session: SessionState; navigate: Navigate; client?: ContractDetailsApi };

export function ContractDetailsPage({ contractId, session, navigate, client = api }: ContractDetailsPageProps) {
  const [state, setState] = useState<AsyncState<ContractHistoryItem | null>>({ status: 'loading' });
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (session.status !== 'authenticated') return;
    let active = true;
    setState({ status: 'loading' });
    client.findMyContractHistoryEntry(contractId).then((entry) => {
      if (!active) return;
      setState(entry ? { status: 'success', data: entry } : { status: 'empty', data: null });
    }).catch((error: unknown) => {
      if (!active) return;
      setState({ status: 'error', error: error instanceof Error ? error.message : 'Сводка недоступна' });
    });
    return () => { active = false; };
  // The API client is injectable for tests and embedded consumers. Depend on
  // the request inputs rather than the containing object identity: callers may
  // create a fresh facade on every render, which must not restart an in-flight
  // history lookup and leave the page stuck in its loading state.
  }, [contractId, reload, session.status]);

  if (session.status !== 'authenticated') {
    return <section className="route-state"><span className="eyebrow">ACCOUNT / PRIVATE CONTRACT</span><h1>Контракт {contractId}</h1><p>Войдите, чтобы искать контракт только в истории этого аккаунта.</p><button className="primary route-state-action" type="button" onClick={() => navigate(`/login?returnTo=${encodeURIComponent(`/contracts/${contractId}`)}`)}>Войти <ArrowRight size={16} /></button></section>;
  }

  const entry = state.status === 'success' ? state.data : null;
  const presentation = entry ? readContractPresentation(entry.contract_id) : null;

  return <>
    <button className="text-button details-back" type="button" onClick={() => navigate('/history')}><ArrowLeft size={15} /> К истории</button>
    <section className="intro details-intro"><div><span className="eyebrow">ACCOUNT / PRIVATE CONTRACT</span><h1>Контракт {contractId}</h1><p>Сводка owner-scoped истории. Она не является публичной проверкой.</p></div></section>
    {state.status === 'loading' && <div className="commerce-state" role="status"><RefreshCw className="spin" size={18} /> Ищем в истории аккаунта…</div>}
    {state.status === 'error' && <div className="commerce-state" role="alert"><strong>Сводка недоступна</strong><button type="button" onClick={() => setReload((value) => value + 1)}>Повторить</button></div>}
    {state.status === 'empty' && <div className="commerce-state"><strong>Контракт не найден в истории этого аккаунта.</strong><span>Это ничего не доказывает о контрактах других аккаунтов.</span></div>}
    {entry && <article className="contract-details panel">
      <dl className="contract-safe-summary"><div><dt>Статус</dt><dd>{contractStatusLabel(entry.status)}</dd></div><div><dt>Создан</dt><dd>{contractDate(entry.created_at)}</dd></div><div><dt>Связь с операцией аккаунта</dt><dd>{entry.ledger_transaction_id === null ? 'Пока не указана' : 'Операция аккаунта связана'}</dd></div></dl>
      {presentation ? <>
        <section className="details-artwork" aria-label="Входные предметы"><h2>Входные предметы</h2><div>{presentation.inputs.map((item, index) => <figure key={`${item.id}-${index}`}><div className="history-art"><SkinImage src={resolveSkinImage(item)} alt={`${item.weapon} | ${item.skin}`} accent={item.color} /></div><figcaption>{item.weapon} | {item.skin}</figcaption></figure>)}</div><p>Суммарное значение при отправке: {microcredits(presentation.inputValueMicrocredits)}</p></section>
        <section className="details-artwork details-result" aria-label="Результат контракта"><h2>Результат</h2>{presentation.result ? <figure><div className="history-art"><SkinImage src={resolveSkinImage(presentation.result)} alt={`${presentation.result.weapon} | ${presentation.result.skin}`} accent={presentation.result.color} /></div><figcaption>{presentation.result.weapon} | {presentation.result.skin}</figcaption></figure> : <p>Artwork результата не был возвращён текущим API.</p>}<p>Значение результата: {microcredits(presentation.resultValueMicrocredits)}</p></section>
      </> : <p className="history-media-unavailable">Текущий API истории не раскрывает состав, artwork или значения этого контракта.</p>}
    </article>}
  </>;
}
