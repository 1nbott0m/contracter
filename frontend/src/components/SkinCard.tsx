import { X } from 'lucide-react';
import type { SkinDefinition } from '../types';
import { resolveSkinImage, SkinImage } from './SkinImage';

type SkinCardProps = {
  item: SkinDefinition;
  selected?: boolean;
  onRemove?: () => void;
  onAdd?: () => void;
  onClick?: () => void;
};

function money(value: number) {
  return `${value.toLocaleString('ru-RU')} CC`;
}

export function SkinCard({ item, selected = false, onRemove, onAdd, onClick }: SkinCardProps) {
  const handleClick = onClick || onAdd;
  return (
    <article className={`skin-card ${selected ? 'selected' : ''}`}>
      <div className="skin-art" style={{ '--accent': item.color } as React.CSSProperties}>
        <SkinImage src={resolveSkinImage(item)} alt={`${item.weapon} | ${item.skin}`} accent={item.color} />
        <span>{item.weapon.split('-')[0]}</span>
        <div className="art-line" />
      </div>
      {selected && onRemove && <button className="remove" aria-label="Удалить" onClick={(event) => { event.stopPropagation(); onRemove(); }}><X size={14} /></button>}
      <div className="skin-meta"><span className="weapon">{item.weapon}</span><strong>{item.skin}</strong><span className="wear">{item.wear}</span></div>
      <div className="card-footer"><span className="rarity" style={{ color: item.color }}>{item.rarity}</span><b>{money(item.price)}</b></div>
      {handleClick && <button className="skin-card-select" type="button" onClick={handleClick} aria-label={`Выбрать ${item.weapon} | ${item.skin}`} />}
    </article>
  );
}

export { money };
