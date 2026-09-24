import type { SkinDefinition } from '../types';
import { resolveSkinImage } from './SkinImage';
import { SkinCard } from './SkinCard';

type PossibleResultsProps = {
  items: SkinDefinition[];
  developmentData?: boolean;
};

export function PossibleResults({ items, developmentData = false }: PossibleResultsProps) {
  const pictured = items.filter((item) => Boolean(resolveSkinImage(item)?.trim()));

  return (
    <section className="content-section possible-results" aria-labelledby="possible-results-heading">
      <div className="section-title">
        <div>
          <span className="eyebrow">UNVERIFIED CATALOG PREVIEW{developmentData ? ' / DEV DATA' : ''}</span>
          <h2 id="possible-results-heading">НЕПРОВЕРЕННЫЙ ПРЕДПРОСМОТР КАТАЛОГА</h2>
          <p className="catalog-preview-disclaimer">Этот клиентский предпросмотр не подтверждает допустимость результата. Авторитетен только серверный quote.</p>
        </div>
      </div>
      {pictured.length > 0
        ? <div className="result-grid">{pictured.map((item) => <SkinCard item={item} key={item.id} />)}</div>
        : <div className="results-empty"><strong>Предпросмотр пока недоступен</strong><span>Добавьте предметы, чтобы увидеть непроверенную выборку из публичного каталога.</span></div>}
    </section>
  );
}
