import { ArrowRight } from 'lucide-react';
import type { MouseEvent } from 'react';
import type { Navigate } from '../router';

export function NotFoundPage({ navigate }: { navigate: Navigate }) {
  const returnToContracts = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    navigate('/contracts');
  };

  return <section className="route-state not-found-page">
    <span className="eyebrow">ERROR / 404</span>
    <h1>Страница не найдена</h1>
    <p>Такого адреса нет или страница была перемещена.</p>
    <a className="primary route-state-action" href="/contracts" onClick={returnToContracts}>К контрактам <ArrowRight size={16} /></a>
  </section>;
}
