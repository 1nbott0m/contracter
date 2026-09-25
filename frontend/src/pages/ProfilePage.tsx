import { ArrowRight, History, LogOut, Package } from 'lucide-react';
import type { Navigate } from '../router';
import type { SessionState } from '../session';

export function ProfilePage({ session, navigate, logout }: { session: SessionState; navigate: Navigate; logout: () => Promise<boolean> }) {
  if (session.status !== 'authenticated') {
    return <section className="route-state"><span className="eyebrow">ACCOUNT / PROFILE</span><h1>Профиль</h1><p>{session.status === 'loading' ? 'Проверяем сессию.' : session.status === 'expired' ? 'Сессия истекла. Войдите снова.' : session.status === 'error' ? 'API сессии недоступен.' : 'Войдите, чтобы открыть данные аккаунта.'}</p>{session.status !== 'loading' && <button className="primary route-state-action" type="button" onClick={() => navigate('/login?returnTo=/profile')}>Войти <ArrowRight size={16} /></button>}</section>;
  }
  return <section className="profile-page"><span className="eyebrow">ACCOUNT / PROFILE</span><h1>ПРОФИЛЬ</h1><div className="profile-account panel"><span className="avatar">{session.account.login.slice(0, 1).toUpperCase()}</span><div><small>ЛОГИН</small><strong>{session.account.login}</strong><span>ID: {session.account.user_id}</span></div></div><div className="profile-shortcuts"><button type="button" onClick={() => navigate('/inventory')}><Package size={19} /><strong>Инвентарь</strong><span>Ваши предметы</span></button><button type="button" onClick={() => navigate('/history')}><History size={19} /><strong>История</strong><span>Операции аккаунта</span></button><button type="button" onClick={async () => { if (await logout()) navigate('/login', { replace: true }); }}><LogOut size={19} /><strong>Выйти</strong><span>Завершить текущую сессию</span></button></div></section>;
}
