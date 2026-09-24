import { useCallback, useEffect, useRef, useState } from 'react';
import { api, type Account, type LoginResponse } from '../api';
import { sessionFailureState, type SessionState } from '../session';

export type SessionApi = {
  me: () => Promise<Account>;
  login: (login: string, password: string) => Promise<LoginResponse>;
  logout: () => Promise<void>;
};

export function useSession(client: SessionApi = api) {
  const [state, setState] = useState<SessionState>({ status: 'loading' });
  const authenticated = useRef(false);

  const refresh = useCallback(async () => {
    setState({ status: 'loading' });
    try {
      const account = await client.me();
      authenticated.current = true;
      setState({ status: 'authenticated', account });
    } catch (error) {
      setState(sessionFailureState(error, authenticated.current));
    }
  }, [client]);

  const login = useCallback(async (loginValue: string, password: string) => {
    setState({ status: 'loading' });
    try {
      await client.login(loginValue, password);
      const account = await client.me();
      authenticated.current = true;
      setState({ status: 'authenticated', account });
    } catch (error) {
      authenticated.current = false;
      setState(sessionFailureState(error, false));
    }
  }, [client]);

  const logout = useCallback(async () => {
    setState({ status: 'loading' });
    try {
      await client.logout();
      authenticated.current = false;
      setState({ status: 'unauthenticated' });
    } catch (error) {
      setState(sessionFailureState(error, authenticated.current));
    }
  }, [client]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { state, refresh, login, logout } as const;
}
