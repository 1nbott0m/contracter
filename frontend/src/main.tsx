import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ArrowRight } from 'lucide-react';
import './styles.css';
import './extra.css';
import './login.css';
import './ux.css';
import './reveal-cinematic.css';
import './visual-polish.css';
import './image-states.css';
import './accessibility.css';
import './contract-builder.css';
import { AppShell, type ApiStatus } from './components/AppShell';
import { ContractsPage } from './pages/ContractsPage';
import { NotFoundPage } from './pages/NotFoundPage';
import { useRouter, type AppRoute, type Navigate } from './router';
import { useSession } from './hooks/useSession';
import type { SessionState } from './session';

const routeCopy: Partial<Record<AppRoute['id'], { eyebrow: string; title: string; text: string }>> = {
  market: { eyebrow: 'MARKET / CC ECONOMY', title: 'Маркет', text: 'Каталог предметов и покупка будут доступны на этой странице.' },
  inventory: { eyebrow: 'COLLECTION / INVENTORY', title: 'Инвентарь', text: 'Ваши предметы и фильтры будут доступны на этой странице.' },
  history: { eyebrow: 'AUDIT TRAIL / HISTORY', title: 'История контрактов', text: 'Завершённые операции будут доступны после загрузки данных аккаунта.' },
  profile: { eyebrow: 'ACCOUNT / PROFILE', title: 'Профиль', text: 'Для доступа к данным аккаунта потребуется авторизация.' },
  transparency: { eyebrow: 'TRANSPARENCY / CONTROL', title: 'Прозрачность', text: 'Здесь будут опубликованы правила контрактов и доступные владельцу способы сверки истории.' },
  terms: { eyebrow: 'INFORMATION / TERMS', title: 'Условия использования', text: 'Актуальные условия сервиса будут опубликованы на этой странице.' },
  privacy: { eyebrow: 'INFORMATION / PRIVACY', title: 'Политика конфиденциальности', text: 'Актуальная информация об обработке данных будет опубликована на этой странице.' },
  support: { eyebrow: 'SERVICE / SUPPORT', title: 'Поддержка', text: 'Контакты и способы обращения будут опубликованы на этой странице.' },
};

function RoutePage({ route, navigate, session, logout }: { route: AppRoute; navigate: Navigate; session: SessionState; logout: () => Promise<boolean> }) {
  if (route.id === 'contract-details') {
    return <section className="route-state"><span className="eyebrow">MY CONTRACT / HISTORY ENTRY</span><h1>Контракт {route.params.contractId}</h1><p>Здесь будет показана доступная владельцу сводка операции из истории аккаунта.</p><button className="primary route-state-action" onClick={() => navigate('/history')}>К истории <ArrowRight size={16} /></button></section>;
  }

  const copy = routeCopy[route.id];
  if (!copy) return null;
  if (route.id === 'profile' && session.status === 'authenticated') {
    return <section className="route-state"><span className="eyebrow">{copy.eyebrow}</span><h1>{copy.title}</h1><p>Вы вошли как {session.account.login}.</p><button className="primary route-state-action" onClick={async () => { if (await logout()) navigate('/login', { replace: true }); }}>Выйти <ArrowRight size={16} /></button></section>;
  }
  return <section className="route-state"><span className="eyebrow">{copy.eyebrow}</span><h1>{copy.title}</h1><p>{copy.text}</p>{route.id === 'profile' && <button className="primary route-state-action" onClick={() => navigate('/login')}>Войти <ArrowRight size={16} /></button>}</section>;
}

function LoginPage({ navigate, login: authenticate }: { navigate: Navigate; login: (login: string, password: string) => Promise<boolean> }) {
  const [login, setLogin] = useState('');
  const [password, setPassword] = useState('');
  const [loginError, setLoginError] = useState('');
  const submitLogin = async (event: React.FormEvent) => {
    event.preventDefault();
    setLoginError('');
    try {
      if (!await authenticate(login, password)) {
        setLoginError('Не удалось войти. Проверьте логин, пароль или доступность API.');
        return;
      }
      navigate('/contracts', { replace: true });
    } catch {
      setLoginError('Не удалось войти. Проверьте логин и пароль.');
    }
  };

  return <form className="login-modal route-login" onSubmit={submitLogin}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Войти</h1><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></label>{loginError && <p className="login-error">{loginError}</p>}<button className="primary" type="submit">ПРОДОЛЖИТЬ <ArrowRight size={16} /></button></form>;
}

export function App() {
  const { route, navigate } = useRouter();
  const [apiStatus, setApiStatus] = useState<ApiStatus>(() => route.id === 'contracts' ? 'connecting' : 'unverified');
  const session = useSession();

  useEffect(() => {
    if (session.state.status === 'loading') setApiStatus('connecting');
    else if (session.state.status === 'authenticated') setApiStatus('live');
    else if (session.state.status === 'unauthenticated' || session.state.status === 'expired') setApiStatus('auth');
    else setApiStatus('fallback');
  }, [session.state]);

  let page;
  if (route.id === 'contracts' && session.state.status === 'authenticated') page = <ContractsPage setApiStatus={setApiStatus} />;
  else if (route.id === 'contracts' && session.state.status === 'loading') page = <section className="route-state"><h1>Проверяем сессию</h1><p>Загружаем данные аккаунта.</p></section>;
  else if (route.id === 'contracts' && session.state.status === 'error') page = <section className="route-state"><h1>API недоступен</h1><p>Не удалось проверить сессию. Повторите попытку.</p><button className="primary route-state-action" onClick={() => void session.refresh()}>Повторить <ArrowRight size={16} /></button></section>;
  else if (route.id === 'contracts') page = <section className="route-state"><h1>{session.state.status === 'expired' ? 'Сессия истекла' : 'Требуется вход'}</h1><p>Войдите, чтобы использовать принадлежащие вам предметы.</p><button className="primary route-state-action" onClick={() => navigate('/login')}>Войти <ArrowRight size={16} /></button></section>;
  else if (route.id === 'login') page = <LoginPage navigate={navigate} login={session.login} />;
  else if (route.id === 'not-found') page = <NotFoundPage navigate={navigate} />;
  else page = <RoutePage route={route} navigate={navigate} session={session.state} logout={session.logout} />;

  return <AppShell route={route} navigate={navigate} apiStatus={apiStatus} sessionState={session.state}>{page}</AppShell>;
}

createRoot(document.getElementById('root')!).render(<App />);
