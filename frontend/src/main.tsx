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
import './layout-fixes.css';
import './contract-builder.css';
import './commerce.css';
import './information.css';
import { AppShell, type ApiStatus } from './components/AppShell';
import { api } from './api';
import { ContractsPage } from './pages/ContractsPage';
import { NotFoundPage } from './pages/NotFoundPage';
import { MarketPage } from './pages/MarketPage';
import { InventoryPage } from './pages/InventoryPage';
import { ProfilePage } from './pages/ProfilePage';
import { HistoryPage } from './pages/HistoryPage';
import { ContractDetailsPage } from './pages/ContractDetailsPage';
import { VerificationPage } from './pages/VerificationPage';
import { TransparencyPage } from './pages/TransparencyPage';
import { TermsPage } from './pages/TermsPage';
import { PrivacyPage } from './pages/PrivacyPage';
import { SupportPage } from './pages/SupportPage';
import { useRouter, type Navigate } from './router';
import { useSession } from './hooks/useSession';
import { ErrorBoundary } from './error-boundary';

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
      const returnTo = new URLSearchParams(window.location.search).get('returnTo');
      navigate(returnTo?.startsWith('/') && !returnTo.startsWith('//') ? returnTo : '/contracts', { replace: true });
    } catch {
      setLoginError('Не удалось войти. Проверьте логин и пароль.');
    }
  };

  return <form className="login-modal route-login" onSubmit={submitLogin}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Войти</h1><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></label>{loginError && <p className="login-error">{loginError}</p>}<button className="primary" type="submit">ПРОДОЛЖИТЬ <ArrowRight size={16} /></button><button className="text-button auth-switch" type="button" onClick={() => navigate('/register')}>Нет аккаунта? Зарегистрироваться</button></form>;
}

function RegisterPage({ navigate }: { navigate: Navigate }) {
  const [login, setLogin] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError('');
    setSubmitting(true);
    try {
      await api.register('', login.trim(), password);
      navigate('/login?registered=1', { replace: true });
    } catch {
      setError('Не удалось зарегистрироваться. Проверьте invitation token, логин и пароль.');
    } finally {
      setSubmitting(false);
    }
  };
  return <form className="login-modal route-login" onSubmit={submit}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Регистрация</h1><p className="auth-note">Создайте аккаунт бесплатно. После регистрации вы сразу сможете войти в CONTRACTER.</p><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required minLength={3} /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="new-password" minLength={8} required /></label>{error && <p className="login-error">{error}</p>}<button className="primary" type="submit" disabled={submitting}>{submitting ? 'СОЗДАНИЕ…' : 'СОЗДАТЬ АККАУНТ'} <ArrowRight size={16} /></button><button className="text-button auth-switch" type="button" onClick={() => navigate('/login')}>Уже есть аккаунт? Войти</button></form>;
}

function AdminPage({ navigate }: { navigate: Navigate }) {
  return <section className="admin-page route-state"><span className="eyebrow">CONTRACTER / CONTROL ROOM</span><h1>Панель администратора</h1><p>Доступ к этой странице определяется ролью аккаунта на сервере. Если у аккаунта нет роли администратора, API отклонит операции с кодом 403.</p><div className="admin-actions"><button className="primary" type="button" onClick={() => navigate('/contracts')}>Вернуться к контрактам <ArrowRight size={16} /></button><button className="text-button" type="button" onClick={() => navigate('/transparency')}>Открыть проверку честности</button></div></section>;
}

export function App() {
  const { route, navigate } = useRouter();
  const [apiStatus, setApiStatus] = useState<ApiStatus>(() => route.id === 'contracts' ? 'connecting' : 'unverified');
  const [balanceMicrocredits, setBalanceMicrocredits] = useState<number | null>(null);
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
  else if (route.id === 'market') page = <MarketPage session={session.state} navigate={navigate} setApiStatus={setApiStatus} onBalanceChange={setBalanceMicrocredits} />;
  else if (route.id === 'inventory') page = <InventoryPage session={session.state} navigate={navigate} setApiStatus={setApiStatus} />;
  else if (route.id === 'history') page = <HistoryPage session={session.state} navigate={navigate} setApiStatus={setApiStatus} />;
  else if (route.id === 'contract-details') page = <ContractDetailsPage contractId={route.params.contractId} session={session.state} navigate={navigate} />;
  else if (route.id === 'profile') page = <ProfilePage session={session.state} navigate={navigate} logout={session.logout} />;
  else if (route.id === 'login') page = <LoginPage navigate={navigate} login={session.login} />;
  else if (route.id === 'register') page = <RegisterPage navigate={navigate} />;
  else if (route.id === 'transparency') page = <TransparencyPage session={session.state} navigate={navigate} />;
  else if (route.id === 'terms') page = <TermsPage />;
  else if (route.id === 'privacy') page = <PrivacyPage />;
  else if (route.id === 'support') page = <SupportPage navigate={navigate} />;
  else if (route.id === 'admin') page = <AdminPage navigate={navigate} />;
  else if (route.id === 'not-found') page = <NotFoundPage navigate={navigate} />;
  else page = <VerificationPage session={session.state} navigate={navigate} />;

  return <AppShell route={route} navigate={navigate} apiStatus={apiStatus} sessionState={session.state} logout={session.logout} balanceMicrocredits={balanceMicrocredits}>{page}</AppShell>;
}

export const appRoot = createRoot(document.getElementById('root')!);
appRoot.render(<ErrorBoundary><App /></ErrorBoundary>);
