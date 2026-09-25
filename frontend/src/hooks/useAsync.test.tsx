// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { expect, it } from 'vitest';
import { useAsync } from './useAsync';

it('exposes a failed async operation as an error state', async () => {
  const { result } = renderHook(() => useAsync(async () => {
    throw new Error('catalog unavailable');
  }));

  await act(async () => {
    await result.current.run();
  });

  expect(result.current.state).toEqual({ status: 'error', error: 'catalog unavailable' });
});
