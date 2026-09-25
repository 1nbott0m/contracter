import { useCallback, useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { api, type ContractHistoryItem, type ServiceHealth } from '../api';
import type { SessionState } from '../session';
import type { AsyncState } from '../types';
import { readContractPresentation } from './ContractReveal';
import { resolveSkinImage, SkinImage } from './SkinImage';
import { StatusChip } from './StatusChip';

export type ApiStatus = 'unverified' | 'connecting' | 'live' | 'auth' | 'fallback';

type LiveActivityApi = {
  serviceHealth: () => Promise<ServiceHealth>;
  history: () => Promise<ContractHistoryItem[]>;
};

type ConnectionState = 'loading' | 'connected' | 'reconnecting' | 'unavailable';

const connectionCopy: Record<ConnectionState, string> = {
  loading: 'ONLINE: ПРОВЕРКА',
  connected: 'ONLINE: ПОДКЛЮЧЕНО',
  reconnecting: 'ONLINE: ПЕРЕПОДКЛЮЧЕНИЕ',
  unavailable: 'ONLINE: НЕДОСТУПНО',
};

const apiCopy: Record<ApiStatus, string> = {
  unverified: 'API NOT CHECKED', connecting: 'API CONNECTING', live: 'API CONNECTED',
  auth: 'AUTHENTICATION REQUIRED', fallback: 'API UNAVAILABLE',
};

export function LiveActivity({ session, client = api, apiStatus = 'unverified' }: { session: SessionState; client?: LiveActivityApi; apiStatus?: ApiStatus }) {
  const [connection, setConnection] = useState<ConnectionState>('loading');
  const [activity, setActivity] = useState<AsyncState<ContractHistoryItem[]>>({ status: 'idle' });

  const checkConnection = useCallback(async (reconnecting = false) => {
    setConnection(reconnecting ? 'reconnecting' : 'loading');
    try {
      const result = await client.serviceHealth();
      if (result.status !== 'live') throw new Error('Unexpected health response');
      setConnection('connected');
    } catch {
      setConnection('unavailable');
    }
  }, [client]);

  useEffect(() => {
    void checkConnection();
  }, [checkConnection]);

  useEffect(() => {
    if (session.status !== 'authenticated') {
      setActivity({ status: 'idle' });
      return;
    }
    let active = true;
    setActivity({ status: 'loading' });
    client.history().then((items) => {
      if (!active) return;
      setActivity(items.length ? { status: 'success', data: items.slice(0, 3) } : { status: 'empty', data: [] });
    }).catch(() => {
      if (active) setActivity({ status: 'error', error: 'История аккаунта недоступна' });
    });
    return () => { active = false; };
  }, [client, session.status]);

  return <section className="live-activity" aria-label="Статус сервиса и активность" data-online-state={connection}>
    <div className="live-status">
      <span className="live-dot" aria-hidden="true" />
      <b>LIVE</b>
      <span className="live-copy">{apiCopy[apiStatus]}</span>
      <StatusChip className="live-count" tone={connection === 'connected' ? 'success' : connection === 'unavailable' ? 'danger' : 'info'}>{connectionCopy[connection]}</StatusChip>
      {connection === 'unavailable' && <button type="button" onClick={() => void checkConnection(true)} aria-label="Повторить подключение"><RefreshCw size={12} /> Повторить</button>}
    </div>
    <div className="live-feed" aria-live="polite">
      {session.status !== 'authenticated' && <span className="live-empty">Публичная лента операций API не предоставляется</span>}
      {session.status === 'authenticated' && activity.status === 'loading' && <span className="live-empty">Загружаем операции аккаунта…</span>}
      {session.status === 'authenticated' && activity.status === 'empty' && <span className="live-empty">В истории аккаунта пока нет подтверждённых операций</span>}
      {session.status === 'authenticated' && activity.status === 'error' && <span className="live-empty">История аккаунта недоступна</span>}
      {activity.status === 'success' && activity.data.map((entry) => {
        const presentation = readContractPresentation(entry.contract_id);
        return <article className="live-operation" key={entry.contract_id}>
          {presentation?.result && <div className="live-operation-art"><SkinImage src={resolveSkinImage(presentation.result)} alt={`${presentation.result.weapon} | ${presentation.result.skin}`} accent={presentation.result.color} layout="inline" width={96} height={60} /></div>}
          <span><strong>{presentation?.result ? `${presentation.result.weapon} | ${presentation.result.skin}` : 'Контракт'}</strong><small>{entry.contract_id}</small></span>
        </article>;
      })}
    </div>
  </section>;
}
