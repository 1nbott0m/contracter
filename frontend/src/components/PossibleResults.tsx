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
          <span className="eyebrow">OUTPUT RANGE{developmentData ? ' / DEV DATA' : ''}</span>
          <h2 id="possible-results-heading">ВОЗМОЖНЫЕ РЕЗУЛЬТАТЫ</h2>
        </div>
      </div>
      {pictured.length > 0
        ? <div className="result-grid">{pictured.map((item) => <SkinCard item={item} key={item.id} />)}</div>
        : <div className="results-empty"><strong>Результаты пока недоступны</strong><span>Добавьте совместимые предметы, чтобы увидеть кандидатов с подтверждёнными изображениями каталога.</span></div>}
    </section>
  );
}
