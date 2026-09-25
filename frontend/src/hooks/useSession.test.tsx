// @vitest-environment jsdom
import { act, renderHook, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ApiRequestError, type Account } from '../api';
import { useSession, type SessionApi } from './useSession';

const account: Account = {
  user_id: 'f7f78654-6db2-49b9-a927-9b75d061523f',
  login: 'tester',
  created_at: '2026-09-24T12:00:00Z',
};

const newerAccount: Account = {
  user_id: '776a43cc-4df9-4666-9d43-1c11b1eaa9b1',
  login: 'new-session',
  created_at: '2026-09-24T12:05:00Z',
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

function sessionApi(me: SessionApi['me']): SessionApi {
  return {
    me,
    login: vi.fn(async () => ({ user_id: account.user_id })),
    logout: vi.fn(async () => undefined),
  };
}

it('moves from loading to authenticated when the session probe succeeds', async () => {
  const client = sessionApi(vi.fn(async () => account));
  const { result } = renderHook(() => useSession(client));

  expect(result.current.state.status).toBe('loading');
  await waitFor(() => expect(result.current.state).toEqual({ status: 'authenticated', account }));
});

it('marks a previously authenticated session as expired after a later 401', async () => {
  const me = vi
    .fn<SessionApi['me']>()
    .mockResolvedValueOnce(account)
    .mockRejectedValueOnce(new ApiRequestError(401, 'UNAUTHORIZED', 'Authentication required'));
  const client = sessionApi(me);
  const { result } = renderHook(() => useSession(client));
  await waitFor(() => expect(result.current.state.status).toBe('authenticated'));

  await act(async () => {
    await result.current.refresh();
  });

  expect(result.current.state).toEqual({ status: 'expired' });
});

it('does not let an older refresh overwrite a newer login', async () => {
  const staleProbe = deferred<Account>();
  const me = vi
    .fn<SessionApi['me']>()
    .mockImplementationOnce(() => staleProbe.promise)
    .mockResolvedValueOnce(newerAccount);
  const client = sessionApi(me);
  const { result } = renderHook(() => useSession(client));

  await act(async () => {
    await result.current.login('new-session', 'secret');
  });
  expect(result.current.state).toEqual({ status: 'authenticated', account: newerAccount });

  await act(async () => {
    staleProbe.resolve(account);
    await staleProbe.promise;
  });

  expect(result.current.state).toEqual({ status: 'authenticated', account: newerAccount });
});

it('does not let an older refresh restore authentication after logout', async () => {
  const staleProbe = deferred<Account>();
  const me = vi
    .fn<SessionApi['me']>()
    .mockResolvedValueOnce(account)
    .mockImplementationOnce(() => staleProbe.promise);
  const client = sessionApi(me);
  const { result } = renderHook(() => useSession(client));
  await waitFor(() => expect(result.current.state.status).toBe('authenticated'));

  let refreshPromise!: Promise<void>;
  act(() => {
    refreshPromise = result.current.refresh();
  });
  await act(async () => {
    await result.current.logout();
  });
  expect(result.current.state).toEqual({ status: 'unauthenticated' });

  await act(async () => {
    staleProbe.resolve(account);
    await refreshPromise;
  });

  expect(result.current.state).toEqual({ status: 'unauthenticated' });
});
