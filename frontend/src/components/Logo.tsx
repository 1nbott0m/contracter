import type { MouseEvent } from 'react';
import type { Navigate } from '../router';

export function Logo({ navigate }: { navigate: Navigate }) {
  const returnToContracts = (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate('/contracts');
  };

  return <a href="/contracts" onClick={returnToContracts} className="brand" aria-label="CONTRACTER — Контракты"><span className="brand-mark"><span>AK</span><i /></span><span>CONTRACTER</span></a>;
}
