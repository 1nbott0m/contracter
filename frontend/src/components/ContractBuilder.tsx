import { useEffect, useMemo, useRef, useState } from 'react';
import { CirclePlus, Sparkles, X } from 'lucide-react';
import type { InventoryItem, SkinDefinition } from '../types';
import { ContractSummary, type ContractSubmitState } from './ContractSummary';
import { ContractReveal, rememberContractPresentation, type ContractRevealState } from './ContractReveal';
import { PossibleResults } from './PossibleResults';
import { resolveSkinImage, SkinImage } from './SkinImage';
import { SkinCard } from './SkinCard';

export const MIN_CONTRACT_ITEMS = 4;
export const MAX_CONTRACT_ITEMS = 10;

export type CommittedContract = {
  contractId: string;
  result?: SkinDefinition | null;
  inputValueMicrocredits?: number | null;
  resultValueMicrocredits?: number | null;
};

type ContractBuilderProps = {
  items: InventoryItem[];
  possibleResults?: SkinDefinition[];
  possibleResultsAreDevelopmentData?: boolean;
  initialSelected?: InventoryItem[];
  selected?: InventoryItem[];
  onSelectionChange?: (items: InventoryItem[]) => void;
  onSubmit: (items: readonly InventoryItem[]) => Promise<CommittedContract>;
};

function distinctSelection(items: InventoryItem[]) {
  return items.filter((item, index, all) => !item.locked && all.findIndex((candidate) => candidate.id === item.id) === index).slice(0, MAX_CONTRACT_ITEMS);
}

export function ContractBuilder({
  items,
  possibleResults = [],
  possibleResultsAreDevelopmentData = false,
  initialSelected = [],
  selected: controlledSelected,
  onSelectionChange,
  onSubmit,
}: ContractBuilderProps) {
  const [internalSelected, setInternalSelected] = useState(() => distinctSelection(initialSelected));
  const [inspected, setInspected] = useState<InventoryItem | null>(null);
  const [submitState, setSubmitState] = useState<ContractSubmitState>({ status: 'idle' });
  const [revealOpen, setRevealOpen] = useState(false);
  const [revealSelection, setRevealSelection] = useState<InventoryItem[]>([]);
  const [revealState, setRevealState] = useState<ContractRevealState>({ status: 'submitting' });
  const submitGeneration = useRef(0);
  const submitting = useRef(false);
  const inspectorTrigger = useRef<HTMLButtonElement | null>(null);
  const inspectorClose = useRef<HTMLButtonElement | null>(null);
  const selected = controlledSelected === undefined ? internalSelected : distinctSelection(controlledSelected);
  const selectedIds = useMemo(() => new Set(selected.map((item) => item.id)), [selected]);

  const updateSelection = (next: InventoryItem[]) => {
    if (submitting.current) return;
    const normalized = distinctSelection(next);
    if (controlledSelected === undefined) setInternalSelected(normalized);
    onSelectionChange?.(normalized);
    setSubmitState({ status: 'idle' });
  };
  const add = (item: InventoryItem) => {
    if (item.locked || selectedIds.has(item.id) || selected.length >= MAX_CONTRACT_ITEMS) return;
    updateSelection([...selected, item]);
  };
  const remove = (item: InventoryItem) => updateSelection(selected.filter((candidate) => candidate.id !== item.id));
  const inspect = (item: InventoryItem, trigger: HTMLButtonElement) => {
    inspectorTrigger.current = trigger;
    setInspected(item);
  };
  const closeInspector = () => {
    setInspected(null);
    queueMicrotask(() => inspectorTrigger.current?.focus());
  };

  useEffect(() => {
    if (!inspected) return;
    inspectorClose.current?.focus();
    const close = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      closeInspector();
    };
    window.addEventListener('keydown', close);
    return () => window.removeEventListener('keydown', close);
  }, [inspected]);

  const submit = async () => {
    if (selected.length < MIN_CONTRACT_ITEMS || selected.length > MAX_CONTRACT_ITEMS || submitting.current) return;
    const generation = ++submitGeneration.current;
    submitting.current = true;
    const submittedSnapshot = Object.freeze([...selected]);
    setSubmitState({ status: 'submitting' });
    setRevealSelection([...submittedSnapshot]);
    setRevealState({ status: 'submitting' });
    setRevealOpen(true);
    try {
      const committed = await onSubmit(submittedSnapshot);
      if (generation !== submitGeneration.current) return;
      rememberContractPresentation(committed.contractId, {
        inputs: [...submittedSnapshot],
        result: committed.result ?? null,
        inputValueMicrocredits: committed.inputValueMicrocredits ?? null,
        resultValueMicrocredits: committed.resultValueMicrocredits ?? null,
      });
      setSubmitState({ status: 'success', contractId: committed.contractId });
      setRevealState({ status: 'success', contractId: committed.contractId, result: committed.result ?? null });
    } catch {
      if (generation !== submitGeneration.current) return;
      setSubmitState({ status: 'error' });
      setRevealState({ status: 'error' });
    } finally {
      if (generation === submitGeneration.current) submitting.current = false;
    }
  };

  const selectionFrozen = submitState.status === 'submitting';

  return (
    <>
      <section className="builder-layout" aria-label="Сборщик контракта">
        <div className="builder panel">
          <div className="section-head"><div><h2>ВАШИ ПРЕДМЕТЫ</h2><span>{selected.length} / 10 ПРЕДМЕТОВ</span></div></div>
          <div className="selection-grid">
            {selected.map((item) => (
              <div className="contract-slot filled-slot" data-testid="contract-slot" key={item.id}>
                <SkinCard item={item} selected onRemove={() => remove(item)} onAdd={() => remove(item)} selectionDisabled={selectionFrozen} />
                <button className="inspect-item" type="button" onClick={(event) => inspect(item, event.currentTarget)} aria-label={`Подробнее о ${item.weapon} | ${item.skin}`}>ПОДРОБНЕЕ</button>
              </div>
            ))}
            {Array.from({ length: MAX_CONTRACT_ITEMS - selected.length }, (_, index) => (
              <div className="contract-slot" data-testid="contract-slot" key={`empty-${index}`}>
                <div className="empty-slot" aria-hidden="true"><CirclePlus size={19} /><span>СВОБОДНЫЙ СЛОТ</span></div>
              </div>
            ))}
          </div>
          <div className="builder-note"><span><Sparkles size={15} /> Эти предметы фиксируются в одном контракте</span><span>Минимум 4 · максимум 10</span></div>
        </div>
        <ContractSummary selected={selected} possibleResultCount={possibleResults.filter((item) => Boolean(resolveSkinImage(item)?.trim())).length} submitState={submitState} onSubmit={submit} />
      </section>

      <section className="content-section inventory contract-inventory" aria-labelledby="contract-inventory-heading">
        <div className="section-title"><div><span className="eyebrow">YOUR COLLECTION</span><h2 id="contract-inventory-heading">ВАШ ИНВЕНТАРЬ</h2></div></div>
        {items.length > 0
          ? <div className="inventory-grid">{items.map((item) => {
              const isSelected = selectedIds.has(item.id);
              return <div className="inventory-choice" key={item.id}><SkinCard item={item} selected={isSelected} onAdd={() => isSelected ? remove(item) : add(item)} selectionDisabled={selectionFrozen || (!isSelected && selected.length >= MAX_CONTRACT_ITEMS)} /><button className="inspect-item" type="button" onClick={(event) => inspect(item, event.currentTarget)} aria-label={`Подробнее о ${item.weapon} | ${item.skin}`}>ПОДРОБНЕЕ</button></div>;
            })}</div>
          : <div className="inventory-empty"><strong>Нет доступных предметов</strong><span>Инвентарь пуст или недоступен. Обновите страницу после входа.</span></div>}
      </section>

      <PossibleResults items={possibleResults} developmentData={possibleResultsAreDevelopmentData} />

      <ContractReveal
        open={revealOpen}
        selected={revealSelection}
        state={revealState}
        onClose={() => setRevealOpen(false)}
        onNew={() => {
          setRevealOpen(false);
          setSubmitState({ status: 'idle' });
          updateSelection([]);
        }}
      />

      {inspected && <div className="item-inspector-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) closeInspector(); }}><section className="item-inspector panel" role="dialog" aria-modal="true" aria-label={`${inspected.weapon} | ${inspected.skin}`}><button ref={inspectorClose} className="inspector-close" type="button" onClick={closeInspector} aria-label="Закрыть"><X size={16} /></button><div className="inspector-art"><SkinImage src={resolveSkinImage(inspected)} alt={`${inspected.weapon} | ${inspected.skin}`} accent={inspected.color} /></div><span className="eyebrow">{inspected.rarity}</span><h2>{inspected.weapon} | {inspected.skin}</h2><p>{inspected.wear}</p></section></div>}
    </>
  );
}
