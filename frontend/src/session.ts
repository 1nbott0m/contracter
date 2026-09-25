import { ApiRequestError, type Account } from './api';

export type SessionState =
  | { status: 'loading' }
  | { status: 'authenticated'; account: Account }
  | { status: 'unauthenticated' }
  | { status: 'expired' }
  | { status: 'error'; error: string };

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Unknown error';
}

export function sessionFailureState(
  error: unknown,
  hadAuthenticatedSession: boolean,
): SessionState {
  if (error instanceof ApiRequestError && error.status === 401) {
    return { status: hadAuthenticatedSession ? 'expired' : 'unauthenticated' };
  }
  return { status: 'error', error: errorMessage(error) };
}
