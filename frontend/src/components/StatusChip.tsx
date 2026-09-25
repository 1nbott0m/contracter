import type { ReactNode } from 'react';

type StatusTone = 'neutral' | 'success' | 'warning' | 'danger' | 'info';

export function StatusChip({ tone = 'neutral', className = '', children }: { tone?: StatusTone; className?: string; children: ReactNode }) {
  return <span className={`status-chip status-chip-${tone} ${className}`.trim()} data-tone={tone} role="status">{children}</span>;
}
