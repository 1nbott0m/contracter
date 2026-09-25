import type { ReactNode } from 'react';

type StatusTone = 'neutral' | 'success' | 'warning' | 'danger' | 'info';

export function StatusChip({ tone = 'neutral', children }: { tone?: StatusTone; children: ReactNode }) {
  return <span className={`status-chip status-chip-${tone}`} data-tone={tone} role="status">{children}</span>;
}
