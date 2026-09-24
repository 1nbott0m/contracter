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
  const operationGeneration = useRef(0);

  const refresh = useCallback(async () => {
    const operation = ++operationGeneration.current;
    setState({ status: 'loading' });
    try {
      const account = await client.me();
      if (operation !== operationGeneration.current) return;
      authenticated.current = true;
      setState({ status: 'authenticated', account });
    } catch (error) {
      if (operation !== operationGeneration.current) return;
      setState(sessionFailureState(error, authenticated.current));
    }
  }, [client]);

  const login = useCallback(async (loginValue: string, password: string) => {
    const operation = ++operationGeneration.current;
    setState({ status: 'loading' });
    try {
      await client.login(loginValue, password);
      const account = await client.me();
      if (operation !== operationGeneration.current) return;
      authenticated.current = true;
      setState({ status: 'authenticated', account });
    } catch (error) {
      if (operation !== operationGeneration.current) return;
      authenticated.current = false;
      setState(sessionFailureState(error, false));
    }
  }, [client]);

  const logout = useCallback(async () => {
    const operation = ++operationGeneration.current;
    setState({ status: 'loading' });
    try {
      await client.logout();
      if (operation !== operationGeneration.current) return;
      authenticated.current = false;
      setState({ status: 'unauthenticated' });
    } catch (error) {
      if (operation !== operationGeneration.current) return;
      setState(sessionFailureState(error, authenticated.current));
    }
  }, [client]);

  useEffect(() => {
    void refresh();
    return () => {
      operationGeneration.current += 1;
    };
  }, [refresh]);

  return { state, refresh, login, logout } as const;
}
