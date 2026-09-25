import type { HTMLAttributes, ReactNode } from 'react';

export function Panel({ children, className = '', ...props }: HTMLAttributes<HTMLElement> & { children: ReactNode }) {
  return <section className={`panel shared-panel ${className}`.trim()} {...props}>{children}</section>;
}
