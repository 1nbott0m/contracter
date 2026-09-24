import type { MouseEvent, ReactNode } from 'react';
import { Search } from 'lucide-react';
import { Logo } from './Logo';
import type { AppRoute, Navigate } from '../router';

type ApiStatus = 'connecting' | 'live' | 'fallback';

type AppShellProps = {
  route: AppRoute;
  navigate: Navigate;
  apiStatus?: ApiStatus;
  children: ReactNode;
};

const primaryNavigation = [
  { id: 'contracts', href: '/contracts', label: 'КОНТРАКТЫ' },
  { id: 'market', href: '/market', label: 'МАРКЕТ' },
  { id: 'inventory', href: '/inventory', label: 'ИНВЕНТАРЬ' },
  { id: 'history', href: '/history', label: 'ИСТОРИЯ' },
] as const;

function activePrimaryRoute(route: AppRoute) {
  return route.id === 'contract-details' ? 'contracts' : route.id;
}

function AppLink({ href, navigate, children, className, ariaLabel, current }: {
  href: string;
  navigate: Navigate;
  children: ReactNode;
  className?: string;
  ariaLabel?: string;
  current?: boolean;
}) {
  const onClick = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(href);
  };

  return <a href={href} onClick={onClick} className={className} aria-label={ariaLabel} aria-current={current ? 'page' : undefined}>{children}</a>;
}

export function AppShell({ route, navigate, apiStatus = 'connecting', children }: AppShellProps) {
  const activeRoute = activePrimaryRoute(route);
  const pageClass = `page-${route.id}`;

  return <div className={`app-shell ${pageClass}`}>
    <div className="live-bar" aria-label="Статус сервиса">
      <span className="live-dot" />
      <b>LIVE</b>
      <span className="live-copy">{apiStatus === 'live' ? 'API CONNECTED · ' : apiStatus === 'fallback' ? 'API UNAVAILABLE · ' : 'API CONNECTING · '}LIVE ACTIVITY</span>
      <span className="live-count">ONLINE: НЕДОСТУПНО</span>
      <span className="live-empty">Новые подтверждённые операции пока недоступны</span>
    </div>

    <header className="header">
      <Logo />
      <nav className="desktop-navigation" aria-label="Основная навигация">
        {primaryNavigation.map((item) => <AppLink href={item.href} navigate={navigate} current={activeRoute === item.id} key={item.id}>{item.label}</AppLink>)}
      </nav>
      <div className="header-actions">
        <AppLink href="/market" navigate={navigate} className="search" ariaLabel="Поиск скина"><Search size={16} /> Поиск скина</AppLink>
        <span className="balance">БАЛАНС НЕДОСТУПЕН</span>
        <AppLink href="/profile" navigate={navigate} className="profile-button" ariaLabel="Открыть профиль">
          <span className="avatar">Y</span><span>ПРОФИЛЬ</span>
        </AppLink>
      </div>
    </header>

    <main>{children}</main>

    <footer>
      <div><strong>CONTRACTER</strong><span><AppLink href="/contracts" navigate={navigate}>Контракты</AppLink> · <AppLink href="/market" navigate={navigate}>Маркет</AppLink> · <AppLink href="/inventory" navigate={navigate}>Инвентарь</AppLink> · <AppLink href="/history" navigate={navigate}>История</AppLink></span></div>
      <div><strong>ИНФОРМАЦИЯ</strong><span><AppLink href="/transparency" navigate={navigate}>Прозрачность</AppLink> · <AppLink href="/support" navigate={navigate}>Поддержка</AppLink></span></div>
      <div><strong>18+</strong><span><AppLink href="/terms" navigate={navigate}>Условия</AppLink> · <AppLink href="/privacy" navigate={navigate}>Privacy</AppLink><br />Независимый сервис, не аффилированный с Valve Corporation или Steam.</span></div>
    </footer>

    <nav className="mobile-navigation" aria-label="Мобильная навигация">
      {primaryNavigation.map((item) => <AppLink href={item.href} navigate={navigate} current={activeRoute === item.id} ariaLabel={`${item.label}, мобильная навигация`} key={item.id}>{item.label}</AppLink>)}
    </nav>
  </div>;
}
