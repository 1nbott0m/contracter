import { ArrowRight } from 'lucide-react';
import type { InventoryItem } from '../types';

export type ContractSubmitState =
  | { status: 'idle' }
  | { status: 'submitting' }
  | { status: 'error' }
  | { status: 'success'; contractId: string };

type ContractSummaryProps = {
  selected: InventoryItem[];
  possibleResultCount: number;
  submitState: ContractSubmitState;
  onSubmit: () => void;
};

function money(value: number | null) {
  return value === null ? 'НЕДОСТУПНО' : `${value.toLocaleString('ru-RU')} CC`;
}

function ctaLabel(count: number, state: ContractSubmitState) {
  if (state.status === 'submitting') return 'ФИКСИРУЕМ…';
  if (state.status === 'success') return 'КОНТРАКТ ПРИНЯТ';
  if (state.status === 'error') return 'ПОВТОРИТЬ';
  if (count < 4) return `ВЫБЕРИТЕ ЕЩЁ ${4 - count}`;
  return 'ЗАКЛЮЧИТЬ КОНТРАКТ';
}

export function ContractSummary({ selected, possibleResultCount, submitState, onSubmit }: ContractSummaryProps) {
  const total = selected.some((item) => item.price === null)
    ? null
    : selected.reduce((sum, item) => sum + (item.price ?? 0), 0);
  const disabled = selected.length < 4 || submitState.status === 'submitting' || submitState.status === 'success';

  return (
    <aside className="summary panel" aria-label="Сводка контракта">
      <span className="eyebrow">CONTRACT / {selected.length >= 4 ? 'READY' : 'DRAFT'}</span>
      <h2>КОНТРАКТ</h2>
      <div className="summary-rows">
        <div><span>СТОИМОСТЬ</span><strong>{money(total)}</strong></div>
        <div><span>ПРЕДМЕТОВ</span><strong>{selected.length} / 10</strong></div>
        <div><span>ПОЗИЦИЙ В ПРЕДПРОСМОТРЕ</span><strong>{possibleResultCount || '—'}</strong></div>
      </div>
      {submitState.status === 'error' && <p className="contract-submit-error" role="alert">Не удалось заключить контракт. Проверьте доступность предметов и повторите.</p>}
      {submitState.status === 'success' && <p className="contract-submit-success" role="status">Контракт {submitState.contractId} принят сервером.</p>}
      <button className="primary" disabled={disabled} onClick={onSubmit}>
        {ctaLabel(selected.length, submitState)} <ArrowRight size={17} />
      </button>
      <small>После подтверждения выбранные предметы будут использованы в контракте. Результат подтверждает только сервер.</small>
    </aside>
  );
}

export { money as contractMoney };
