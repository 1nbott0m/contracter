import { X } from 'lucide-react';
import type { SkinDefinition } from '../types';
import { resolveSkinImage, SkinImage } from './SkinImage';

type SkinCardProps = {
  item: SkinDefinition & { locked?: boolean };
  selected?: boolean;
  onRemove?: () => void;
  onAdd?: () => void;
  onClick?: () => void;
};

function money(value: number | null) {
  return value === null ? 'ЦЕНА НЕДОСТУПНА' : `${value.toLocaleString('ru-RU')} CC`;
}

export function SkinCard({ item, selected = false, onRemove, onAdd, onClick }: SkinCardProps) {
  const handleClick = onClick || onAdd;
  const itemName = `${item.weapon} | ${item.skin}`;
  const actionLabel = item.locked
    ? `${itemName} недоступен: предмет заблокирован`
    : selected
      ? `Убрать ${itemName} из контракта`
      : `Выбрать ${itemName} для контракта`;
  return (
    <article className={`skin-card ${selected ? 'selected' : ''} ${item.locked ? 'locked' : ''}`.trim()}>
      <div className="skin-art" style={{ '--accent': item.color } as React.CSSProperties}>
        <SkinImage src={resolveSkinImage(item)} alt={`${item.weapon} | ${item.skin}`} accent={item.color} />
        <span>{item.weapon.split('-')[0]}</span>
        <div className="art-line" />
      </div>
      {selected && onRemove && <button className="remove" aria-label="Удалить" onClick={(event) => { event.stopPropagation(); onRemove(); }}><X size={14} /></button>}
      <div className="skin-meta"><span className="weapon">{item.weapon}</span><strong>{item.skin}</strong><span className="wear">{item.wear}</span></div>
      <div className="card-footer"><span className="rarity" style={{ color: item.color }}>{item.rarity}</span><b>{money(item.price)}</b></div>
      {handleClick && <button className="skin-card-select" type="button" onClick={handleClick} aria-label={actionLabel} aria-pressed={selected} disabled={item.locked} />}
    </article>
  );
}

export { money };
