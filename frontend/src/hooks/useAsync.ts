import { useCallback, useRef, useState } from 'react';
import type { AsyncState } from '../types';
import { errorMessage } from '../session';

type UseAsyncOptions<T> = {
  isEmpty?: (data: T) => boolean;
};

function stateData<T>(state: AsyncState<T>): T | undefined {
  return 'data' in state ? state.data : undefined;
}

function defaultIsEmpty<T>(data: T): boolean {
  return Array.isArray(data) && data.length === 0;
}

export function useAsync<T>(operation: () => Promise<T>, options: UseAsyncOptions<T> = {}) {
  const [state, setState] = useState<AsyncState<T>>({ status: 'idle' });
  const generation = useRef(0);
  const isEmpty = options.isEmpty ?? defaultIsEmpty;

  const run = useCallback(async (): Promise<T | undefined> => {
    const currentGeneration = ++generation.current;
    setState((current) => ({ status: 'loading', data: stateData(current) }));
    try {
      const data = await operation();
      if (generation.current === currentGeneration) {
        setState(isEmpty(data) ? { status: 'empty', data } : { status: 'success', data });
      }
      return data;
    } catch (error) {
      if (generation.current === currentGeneration) {
        setState((current) => ({
          status: 'error',
          error: errorMessage(error),
          data: stateData(current),
        }));
      }
      return undefined;
    }
  }, [isEmpty, operation]);

  return { state, run } as const;
}
