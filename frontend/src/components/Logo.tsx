import type { MouseEvent } from 'react';
import type { Navigate } from '../router';

export function Logo({ navigate }: { navigate: Navigate }) {
  const returnToContracts = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate('/contracts');
  };

  return <a href="/contracts" onClick={returnToContracts} className="brand" aria-label="CONTRACTER — Контракты"><img className="brand-mark" src="/contracter-logo.svg" alt="" width="32" height="32" /><span className="brand-label">CONTRACTER</span></a>;
}
