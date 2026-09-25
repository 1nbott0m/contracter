import { useState, type FormEvent, type MouseEvent } from 'react';
import { ArrowRight, Search } from 'lucide-react';
import { api, type ContractHistoryItem } from '../api';
import type { Navigate } from '../router';
import type { SessionState } from '../session';

type VerificationApi = Pick<typeof api, 'findMyContractHistoryEntry'>;
type VerificationPageProps = { session: SessionState; navigate: Navigate; client?: VerificationApi; embedded?: boolean };
type LookupState = { status: 'idle' | 'loading' | 'missing' | 'error' } | { status: 'found'; entry: ContractHistoryItem };

export function VerificationPage({ session, navigate, client = api, embedded = false }: VerificationPageProps) {
  const [contractId, setContractId] = useState('');
  const [state, setState] = useState<LookupState>({ status: 'idle' });

  if (session.status !== 'authenticated') {
    return <section className={embedded ? 'verification-card panel' : 'route-state'}>{!embedded && <><span className="eyebrow">TRANSPARENCY / ACCOUNT HISTORY</span><h1>Прозрачность</h1></>}<p>Войдите, чтобы искать ID только среди контрактов этого аккаунта.</p>{session.status !== 'loading' && <button className="primary route-state-action" type="button" onClick={() => navigate('/login?returnTo=/transparency')}>Войти <ArrowRight size={16} /></button>}</section>;
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const normalized = contractId.trim();
    if (!normalized || state.status === 'loading') return;
    setState({ status: 'loading' });
    try {
      const entry = await client.findMyContractHistoryEntry(normalized);
      setState(entry ? { status: 'found', entry } : { status: 'missing' });
    } catch {
      setState({ status: 'error' });
    }
  };

  const open = (event: MouseEvent<HTMLAnchorElement>, id: string) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(`/contracts/${encodeURIComponent(id)}`);
  };

  return <>
    {!embedded && <section className="intro commerce-intro"><div><span className="eyebrow">TRANSPARENCY / ACCOUNT HISTORY</span><h1>Прозрачность</h1><p>Найдите контракт среди owner-scoped записей текущего аккаунта.</p></div></section>}
    <section className="verification-card panel">
      <form onSubmit={submit}><label htmlFor="contract-history-id">ID контракта</label><div><input id="contract-history-id" value={contractId} onChange={(event) => setContractId(event.target.value)} autoComplete="off" required placeholder="UUID контракта" /><button className="primary" type="submit" disabled={state.status === 'loading'}><Search size={16} /> {state.status === 'loading' ? 'Ищем…' : 'Найти в моей истории'}</button></div></form>
      <p className="verification-boundary">Это не публичная или криптографическая проверка.</p>
      {state.status === 'found' && <div className="verification-result" role="status"><strong>Контракт найден в истории вашего аккаунта.</strong><a href={`/contracts/${encodeURIComponent(state.entry.contract_id)}`} onClick={(event) => open(event, state.entry.contract_id)}>Открыть сводку <ArrowRight size={15} /></a></div>}
      {state.status === 'missing' && <p className="verification-result" role="status">Контракт не найден в истории этого аккаунта.</p>}
      {state.status === 'error' && <p className="verification-result error" role="alert">Не удалось загрузить историю. Повторите попытку.</p>}
    </section>
  </>;
}
