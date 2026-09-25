// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { matchRoute, useRouter } from './router';

describe('matchRoute', () => {
  it.each([
    ['/contracts', 'contracts'],
    ['/market', 'market'],
    ['/inventory', 'inventory'],
    ['/history', 'history'],
    ['/profile', 'profile'],
    ['/login', 'login'],
    ['/transparency', 'transparency'],
    ['/terms', 'terms'],
    ['/privacy', 'privacy'],
    ['/support', 'support'],
  ])('matches the direct URL %s', (pathname, id) => {
    expect(matchRoute(pathname)).toMatchObject({ id, pathname });
  });

  it('treats the root and trailing slash as the contracts route', () => {
    expect(matchRoute('/')).toMatchObject({ id: 'contracts', pathname: '/contracts' });
    expect(matchRoute('/contracts/')).toMatchObject({ id: 'contracts', pathname: '/contracts' });
  });

  it('extracts a decoded contract id', () => {
    expect(matchRoute('/contracts/CTR-8F4A91')).toEqual({
      id: 'contract-details',
      pathname: '/contracts/CTR-8F4A91',
      params: { contractId: 'CTR-8F4A91' },
    });
  });

  it('returns a real not-found route for unknown or nested paths', () => {
    expect(matchRoute('/unknown')).toMatchObject({ id: 'not-found', pathname: '/unknown' });
    expect(matchRoute('/market/extra')).toMatchObject({ id: 'not-found', pathname: '/market/extra' });
  });
});

describe('useRouter', () => {
  it('navigates without a reload and responds to browser popstate', () => {
    window.history.replaceState({}, '', '/contracts');
    const { result } = renderHook(() => useRouter());

    act(() => result.current.navigate('/market'));
    expect(window.location.pathname).toBe('/market');
    expect(result.current.route.id).toBe('market');

    act(() => {
      window.history.replaceState({}, '', '/privacy');
      window.dispatchEvent(new PopStateEvent('popstate'));
    });
    expect(result.current.route.id).toBe('privacy');
  });
});
