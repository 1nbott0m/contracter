import { describe, expect, it } from 'vitest';
import { ApiRequestError } from './api';
import { sessionFailureState } from './session';

describe('session failure states', () => {
  it('classifies the first unauthorized probe as unauthenticated', () => {
    expect(sessionFailureState(new ApiRequestError(401, 'UNAUTHORIZED', 'Authentication required'), false)).toEqual({
      status: 'unauthenticated',
    });
  });

  it('classifies an unauthorized probe after authentication as expired', () => {
    expect(sessionFailureState(new ApiRequestError(401, 'UNAUTHORIZED', 'Authentication required'), true)).toEqual({
      status: 'expired',
    });
  });

  it('keeps transport and server failures distinct from authentication', () => {
    expect(sessionFailureState(new Error('network down'), false)).toEqual({
      status: 'error',
      error: 'network down',
    });
  });
});
