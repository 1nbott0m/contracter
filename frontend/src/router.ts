import { useCallback, useEffect, useState } from 'react';

export type StaticRouteId =
  | 'contracts'
  | 'market'
  | 'inventory'
  | 'history'
  | 'profile'
  | 'login'
  | 'register'
  | 'transparency'
  | 'terms'
  | 'privacy'
  | 'support';

export type AppRoute =
  | { id: StaticRouteId; pathname: string }
  | { id: 'contract-details'; pathname: string; params: { contractId: string } }
  | { id: 'not-found'; pathname: string };

export type Navigate = (pathname: string, options?: { replace?: boolean }) => void;

const staticRoutes = new Map<string, StaticRouteId>([
  ['/contracts', 'contracts'],
  ['/market', 'market'],
  ['/inventory', 'inventory'],
  ['/history', 'history'],
  ['/profile', 'profile'],
  ['/login', 'login'],
  ['/register', 'register'],
  ['/transparency', 'transparency'],
  ['/terms', 'terms'],
  ['/privacy', 'privacy'],
  ['/support', 'support'],
]);

function normalizePathname(pathname: string) {
  if (pathname === '/') return '/contracts';
  return pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname;
}

export function matchRoute(pathname: string): AppRoute {
  const normalized = normalizePathname(pathname);
  const staticRoute = staticRoutes.get(normalized);
  if (staticRoute) return { id: staticRoute, pathname: normalized };

  const contractMatch = normalized.match(/^\/contracts\/([^/]+)$/);
  if (contractMatch) {
    try {
      return {
        id: 'contract-details',
        pathname: normalized,
        params: { contractId: decodeURIComponent(contractMatch[1]) },
      };
    } catch {
      return { id: 'not-found', pathname: normalized };
    }
  }

  return { id: 'not-found', pathname: normalized };
}

export function useRouter() {
  const [route, setRoute] = useState(() => matchRoute(window.location.pathname));

  useEffect(() => {
    const updateRoute = () => setRoute(matchRoute(window.location.pathname));
    window.addEventListener('popstate', updateRoute);
    return () => window.removeEventListener('popstate', updateRoute);
  }, []);

  const navigate = useCallback<Navigate>((pathname, options) => {
    const next = new URL(pathname, window.location.href);
    if (next.origin !== window.location.origin) {
      window.location.assign(next.href);
      return;
    }

    window.history[options?.replace ? 'replaceState' : 'pushState']({}, '', `${next.pathname}${next.search}${next.hash}`);
    setRoute(matchRoute(next.pathname));
  }, []);

  return { route, navigate };
}
