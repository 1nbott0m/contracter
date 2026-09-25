// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { StatusChip } from './StatusChip';

afterEach(cleanup);

it('exposes semantic status text and tone without relying on color alone', () => {
  render(<StatusChip tone="success">API CONNECTED</StatusChip>);
  const chip = screen.getByText('API CONNECTED');
  expect(chip.getAttribute('data-tone')).toBe('success');
  expect(chip.getAttribute('role')).toBe('status');
});
