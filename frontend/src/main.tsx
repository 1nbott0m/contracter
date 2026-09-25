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
  const registered = new URLSearchParams(window.location.search).get('registered') === '1';
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

  return <form className="login-modal route-login" onSubmit={submitLogin}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Войти</h1>{registered && <p className="auth-success" role="status">Аккаунт создан. Теперь войдите с выбранным паролем.</p>}<label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></label>{loginError && <p className="login-error">{loginError}</p>}<button className="primary" type="submit">ПРОДОЛЖИТЬ <ArrowRight size={16} /></button><button className="secondary-auth" type="button" onClick={() => window.location.assign('https://contracter.onrender.com/api/v1/auth/steam/start')}>ВОЙТИ ЧЕРЕЗ STEAM</button><button className="text-button auth-switch" type="button" onClick={() => navigate('/register')}>Нет аккаунта? Зарегистрироваться</button></form>;
}

function RegisterPage({ navigate }: { navigate: Navigate }) {
  const [login, setLogin] = useState('');
  const [password, setPassword] = useState('');
  const [passwordConfirmation, setPasswordConfirmation] = useState('');
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError('');
    if (password !== passwordConfirmation) {
      setError('Пароли не совпадают. Проверьте оба поля.');
      return;
    }
    setSubmitting(true);
    try {
      await api.register('', login.trim(), password);
      navigate('/login?registered=1', { replace: true });
    } catch {
      setError('Не удалось зарегистрироваться. Проверьте логин и пароль или попробуйте другой логин.');
    } finally {
      setSubmitting(false);
    }
  };
  return <form className="login-modal route-login" onSubmit={submit}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Регистрация</h1><p className="auth-note">Создайте аккаунт бесплатно. После регистрации вы сразу сможете войти в CONTRACTER.</p><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required minLength={3} /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="new-password" minLength={8} required /></label><label>ПОВТОРИТЕ ПАРОЛЬ<input type="password" value={passwordConfirmation} onChange={(event) => setPasswordConfirmation(event.target.value)} autoComplete="new-password" minLength={8} required /></label>{error && <p className="login-error" role="alert">{error}</p>}<button className="primary" type="submit" disabled={submitting}>{submitting ? 'СОЗДАНИЕ…' : 'СОЗДАТЬ АККАУНТ'} <ArrowRight size={16} /></button><button className="text-button auth-switch" type="button" onClick={() => navigate('/login')}>Уже есть аккаунт? Войти</button></form>;
}

function AdminPage({ navigate, session }: { navigate: Navigate; session: ReturnType<typeof useSession>['state'] }) {
  const [verified, setVerified] = useState<'idle' | 'checking' | 'ok' | 'denied' | 'error'>('idle');
  const [totpVerified, setTotpVerified] = useState(false);
  const [secret, setSecret] = useState('');
  const [code, setCode] = useState('');
  const [totpError, setTotpError] = useState('');
  const [dashboard, setDashboard] = useState<Awaited<ReturnType<typeof api.adminDashboard>> | null>(null);
  useEffect(() => {
    if (session.status !== 'authenticated' || !session.account.is_admin) return;
    setVerified('checking');
    void api.adminMe().then((result) => { setTotpVerified(result.totp_verified); setVerified('ok'); }).catch((error) => {
      setVerified(error instanceof Error && 'status' in error && (error as { status?: number }).status === 403 ? 'denied' : 'error');
    });
    void api.adminDashboard().then(setDashboard).catch(() => setDashboard(null));
  }, [session]);
  if (session.status === 'loading') return <section className="route-state"><h1>Проверяем доступ</h1><p>Загружаем роль аккаунта.</p></section>;
  if (session.status !== 'authenticated') return <section className="route-state"><h1>Войдите в аккаунт</h1><p>Панель администратора доступна только авторизованным пользователям.</p><button className="primary" type="button" onClick={() => navigate('/login?returnTo=/admin')}>Войти <ArrowRight size={16} /></button></section>;
  if (!session.account.is_admin || verified === 'denied') return <section className="route-state"><h1>Доступ закрыт</h1><p>У аккаунта «{session.account.login}» нет активной роли администратора.</p><button className="primary" type="button" onClick={() => navigate('/contracts')}>Вернуться к контрактам <ArrowRight size={16} /></button></section>;
  if (verified === 'error') return <section className="route-state"><h1>Панель временно недоступна</h1><p>Сервер не подтвердил административную сессию. Повторите попытку позже.</p></section>;
  const setup = async () => { setTotpError(''); try { const result = await api.provisionTotp(); setSecret(result.secret); } catch { setTotpError('Не удалось создать секрет 2FA.'); } };
  const confirm = async (event: React.FormEvent) => { event.preventDefault(); setTotpError(''); try { await api.verifyTotp(code); setTotpVerified(true); } catch { setTotpError('Неверный код Google Authenticator.'); } };
  return <section className="admin-page route-state"><span className="eyebrow">CONTRACTER / CONTROL ROOM</span><h1>Панель администратора</h1><p>Административная роль подтверждена сервером. Аккаунт: <strong>{session.account.login}</strong>.</p><div className="admin-status">{totpVerified ? '2FA ACTIVE' : '2FA REQUIRED'}</div>{!totpVerified && <div className="panel admin-2fa"><h2>Google Authenticator</h2><p>Подключите TOTP перед выполнением административных операций.</p>{!secret ? <button className="primary" type="button" onClick={() => void setup()}>Создать секрет 2FA</button> : <><p>Добавьте секрет в приложение: <code>{secret}</code></p><form onSubmit={confirm}><input value={code} onChange={(event) => setCode(event.target.value)} inputMode="numeric" pattern="[0-9]{6}" placeholder="123456" aria-label="Код Google Authenticator" required /><button className="primary" type="submit">Подтвердить код</button></form></>}{totpError && <p className="login-error" role="alert">{totpError}</p>}</div>}<div className="admin-actions"><button className="primary" type="button" onClick={() => navigate('/transparency')}>Проверка честности <ArrowRight size={16} /></button><button className="text-button" type="button" onClick={() => navigate('/market')}>Открыть маркет</button></div></section>;
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
  else if (route.id === 'admin') page = <AdminPage navigate={navigate} session={session.state} />;
  else if (route.id === 'not-found') page = <NotFoundPage navigate={navigate} />;
  else page = <VerificationPage session={session.state} navigate={navigate} />;

  return <AppShell route={route} navigate={navigate} apiStatus={apiStatus} sessionState={session.state} logout={session.logout} balanceMicrocredits={balanceMicrocredits}>{page}</AppShell>;
}

export const appRoot = createRoot(document.getElementById('root')!);
appRoot.render(<ErrorBoundary><App /></ErrorBoundary>);
