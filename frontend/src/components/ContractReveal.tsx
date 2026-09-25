import { useEffect, useState } from 'react';
import { ArrowRight, X } from 'lucide-react';
import type { InventoryItem, SkinDefinition } from '../types';
import { resolveSkinImage, SkinImage } from './SkinImage';

const PRESENTATION_PREFIX = 'contracter.contract.presentation.';
const FLOW = ['SELECT', 'LOCK', 'SIGN', 'MERGE', 'RESULT'] as const;

export type ContractPresentation = {
  inputs: InventoryItem[];
  result: SkinDefinition | null;
  inputValueMicrocredits: number | null;
  resultValueMicrocredits: number | null;
};

export type ContractRevealState =
  | { status: 'submitting' }
  | { status: 'success'; contractId: string; result: SkinDefinition | null }
  | { status: 'error' };

function isSkin(value: unknown): value is SkinDefinition {
  if (!value || typeof value !== 'object') return false;
  const item = value as Partial<SkinDefinition>;
  return typeof item.id === 'string'
    && typeof item.weapon === 'string'
    && typeof item.skin === 'string'
    && typeof item.wear === 'string'
    && (typeof item.price === 'number' || item.price === null)
    && typeof item.color === 'string'
    && typeof item.rarity === 'string'
    && typeof item.image === 'string';
}

function presentationKey(contractId: string) {
  return `${PRESENTATION_PREFIX}${contractId.trim().toLowerCase()}`;
}

/**
 * Keeps display-only data from a successful, server-backed flow in this tab.
 * A stored presentation is never considered proof that a contract exists:
 * history/details render it only after the owner-scoped history API matches.
 */
export function rememberContractPresentation(contractId: string, presentation: ContractPresentation) {
  try {
    sessionStorage.setItem(presentationKey(contractId), JSON.stringify(presentation));
  } catch {
    // Storage can be disabled. The server result remains authoritative.
  }
}

export function readContractPresentation(contractId: string): ContractPresentation | null {
  try {
    const raw = sessionStorage.getItem(presentationKey(contractId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<ContractPresentation>;
    if (!Array.isArray(parsed.inputs) || !parsed.inputs.every(isSkin)) return null;
    if (parsed.result !== null && !isSkin(parsed.result)) return null;
    const validMoney = (value: unknown) => value === null || (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0);
    if (!validMoney(parsed.inputValueMicrocredits) || !validMoney(parsed.resultValueMicrocredits)) return null;
    return parsed as ContractPresentation;
  } catch {
    return null;
  }
}

function reducedMotionPreferred() {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

type ContractRevealProps = {
  open: boolean;
  selected: readonly InventoryItem[];
  state: ContractRevealState;
  onClose: () => void;
  onNew: () => void;
};

export function ContractReveal({ open, selected, state, onClose, onNew }: ContractRevealProps) {
  const reducedMotion = reducedMotionPreferred();
  const [stage, setStage] = useState(() => reducedMotion ? 3 : 0);

  useEffect(() => {
    if (!open) return;
    if (reducedMotion) {
      setStage(state.status === 'success' ? 4 : 3);
      return;
    }
    if (state.status === 'error') return;
    const target = state.status === 'success' ? 4 : 3;
    if (stage >= target) return;
    const timer = window.setTimeout(() => setStage((current) => Math.min(current + 1, target)), 1_150);
    return () => window.clearTimeout(timer);
  }, [open, reducedMotion, stage, state.status]);

  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && state.status !== 'submitting') onClose();
    };
    window.addEventListener('keydown', close);
    return () => window.removeEventListener('keydown', close);
  }, [onClose, open, state.status]);

  if (!open) return null;
  const showResult = state.status === 'success' && stage === 4;
  const skipReveal = () => setStage(state.status === 'success' ? 4 : 3);

  return <div className="reveal-backdrop" role="presentation">
    <section className={`reveal-panel cinematic-reveal ${showResult ? 'is-result' : state.status === 'error' ? 'is-error' : 'is-processing'}`} role="dialog" aria-modal="true" aria-label="Фиксация контракта">
      <div className="reveal-beam" aria-hidden="true" />
      <div className="reveal-orbit orbit-one" aria-hidden="true" />
      <div className="reveal-orbit orbit-two" aria-hidden="true" />
      {state.status !== 'submitting' && <button className="reveal-close" type="button" onClick={onClose} aria-label="Закрыть"><X size={17} /></button>}
      <span className="eyebrow">CONTRACTER / {state.status === 'error' ? 'NOT COMMITTED' : FLOW[stage]}</span>
      {state.status === 'error' ? <>
        <h2>КОНТРАКТ НЕ ПОДТВЕРЖДЁН</h2>
        <p role="alert">Контракт не подтверждён сервером. Выбранные предметы не заменены придуманным результатом.</p>
        <button className="primary" type="button" onClick={onClose}>ВЕРНУТЬСЯ</button>
      </> : showResult ? <>
        <span className="result-kicker">SERVER RESULT</span>
        <h2>КОНТРАКТ ПРИНЯТ</h2>
        {state.result ? <article className="result-skin">
          <div className="result-art"><SkinImage src={resolveSkinImage(state.result)} alt={`${state.result.weapon} | ${state.result.skin}`} accent={state.result.color} loading="eager" /></div>
          <div className="result-copy"><b>{state.result.weapon}</b><strong>{state.result.skin}</strong><span>{state.result.wear}</span></div>
        </article> : <p className="result-unavailable">Контракт принят, но текущий API не вернул artwork результата. Результат не подменён предпросмотром.</p>}
        <p className="reveal-contract-id">Контракт: {state.contractId}</p>
        <button className="primary" type="button" onClick={onNew}>СОБРАТЬ НОВЫЙ <ArrowRight size={16} /></button>
        <button className="close-login" type="button" onClick={onClose}>ЗАКРЫТЬ</button>
      </> : <>
        <h2>ФИКСИРУЕМ КОНТРАКТ</h2>
        <ol className="reveal-flow" aria-label="Этапы фиксации контракта">
          {FLOW.map((label, index) => <li key={label} className={index < stage ? 'complete' : index === stage ? 'active' : ''}>{label}</li>)}
        </ol>
        <div className="reveal-items" aria-label="Выбранные предметы">
          {selected.map((item) => <span className="contract-panel" key={item.id}>
            <SkinImage src={resolveSkinImage(item)} alt={`${item.weapon} | ${item.skin}`} accent={item.color} loading="eager" layout="inline" width={320} height={200} />
            <small>{item.weapon}<br /><b>{item.skin}</b></small><i aria-hidden="true" />
          </span>)}
        </div>
        <div className="reveal-core" aria-hidden="true"><span className="core-ring" /><span className="core-mark">{String(stage + 1).padStart(2, '0')}</span></div>
        <div className="reveal-progress" aria-hidden="true"><span /></div>
        <p>Сервер фиксирует выбранные предметы. Результат появится только после успешного ответа.</p>
        <button className="reveal-skip" type="button" onClick={skipReveal}>Пропустить анимацию</button>
      </>}
    </section>
  </div>;
}
