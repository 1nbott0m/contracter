import { useEffect, useRef, type MouseEvent, type ReactNode } from 'react';
import { Logo } from './Logo';
import { GlobalSearch } from './GlobalSearch';
import { ProfileMenu } from './ProfileMenu';
import { Footer } from './Footer';
import { LiveActivity, type ApiStatus } from './LiveActivity';
import type { AppRoute, Navigate } from '../router';
import type { SessionState } from '../session';

export type { ApiStatus } from './LiveActivity';

type AppShellProps = {
  route: AppRoute;
  navigate: Navigate;
  apiStatus?: ApiStatus;
  sessionState?: SessionState;
  logout?: () => Promise<boolean>;
  balanceMicrocredits?: number | null;
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

export function AppShell({ route, navigate, apiStatus = 'unverified', sessionState = { status: 'unauthenticated' }, logout = async () => false, balanceMicrocredits = null, children }: AppShellProps) {
  const activeRoute = activePrimaryRoute(route);
  const pageClass = `page-${route.id}`;
  const mainRef = useRef<HTMLElement>(null);
  useEffect(() => {
    const heading = mainRef.current?.querySelector<HTMLElement>('h1');
    document.title = heading?.textContent ? `${heading.textContent} · CONTRACTER` : 'CONTRACTER';
    if (heading) {
      heading.tabIndex = -1;
      heading.focus();
    } else {
      mainRef.current?.focus();
    }
  }, [route.pathname]);

  return <div className={`app-shell ${pageClass}`}>
    <LiveActivity session={sessionState} apiStatus={apiStatus} />

    <header className="header" data-design-system="contracter-neon">
      <Logo navigate={navigate} />
      <nav className="desktop-navigation" aria-label="Основная навигация">
        {primaryNavigation.map((item) => <AppLink href={item.href} navigate={navigate} current={activeRoute === item.id} key={item.id}>{item.label}</AppLink>)}
      </nav>
      <div className="header-actions">
        <GlobalSearch navigate={navigate} />
        <span className="balance">{balanceMicrocredits === null ? 'БАЛАНС НЕДОСТУПЕН' : `${(balanceMicrocredits / 1_000_000).toLocaleString('ru-RU')} CC`}</span>
        <ProfileMenu session={sessionState} navigate={navigate} logout={logout} />
      </div>
    </header>

    <main ref={mainRef} className="app-main" tabIndex={-1}>{children}</main>

    <Footer navigate={navigate} />

    <nav className="mobile-navigation" aria-label="Мобильная навигация">
      {primaryNavigation.map((item) => <AppLink href={item.href} navigate={navigate} current={activeRoute === item.id} ariaLabel={`${item.label}, мобильная навигация`} key={item.id}>{item.label}</AppLink>)}
    </nav>
  </div>;
}
