// @vitest-environment jsdom
import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SkinImage } from './SkinImage';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('SkinImage', () => {
  it('shows a null-source fallback without an endless loading spinner', () => {
    const { container } = render(<SkinImage src={null} alt="Unavailable skin" />);

    expect(container.firstElementChild?.getAttribute('data-image-state')).toBe('error');
    expect(container.querySelector('.skin-image-loading')).toBeNull();
  });

  it('recognizes an already complete cached image as ready', async () => {
    vi.spyOn(HTMLImageElement.prototype, 'complete', 'get').mockReturnValue(true);
    vi.spyOn(HTMLImageElement.prototype, 'naturalWidth', 'get').mockReturnValue(640);

    const { container } = render(<SkinImage src="https://cdn.example.test/cached.png" alt="Cached skin" />);

    await waitFor(() => {
      expect(container.firstElementChild?.getAttribute('data-image-state')).toBe('ready');
    });
  });
});
