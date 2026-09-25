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
import './commerce.css';
import './information.css';
import { AppShell, type ApiStatus } from './components/AppShell';
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

  return <form className="login-modal route-login" onSubmit={submitLogin}><span className="eyebrow">CONTRACTER / ACCOUNT</span><h1>Войти</h1><label>ЛОГИН<input value={login} onChange={(event) => setLogin(event.target.value)} autoComplete="username" required /></label><label>ПАРОЛЬ<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required /></label>{loginError && <p className="login-error">{loginError}</p>}<button className="primary" type="submit">ПРОДОЛЖИТЬ <ArrowRight size={16} /></button></form>;
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
  else if (route.id === 'transparency') page = <TransparencyPage session={session.state} navigate={navigate} />;
  else if (route.id === 'terms') page = <TermsPage />;
  else if (route.id === 'privacy') page = <PrivacyPage />;
  else if (route.id === 'support') page = <SupportPage navigate={navigate} />;
  else if (route.id === 'not-found') page = <NotFoundPage navigate={navigate} />;
  else page = <VerificationPage session={session.state} navigate={navigate} />;

  return <AppShell route={route} navigate={navigate} apiStatus={apiStatus} sessionState={session.state} logout={session.logout} balanceMicrocredits={balanceMicrocredits}>{page}</AppShell>;
}

export const appRoot = createRoot(document.getElementById('root')!);
appRoot.render(<App />);
