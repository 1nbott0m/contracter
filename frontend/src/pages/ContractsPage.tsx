import { useEffect, useMemo, useRef, useState } from 'react';
import { ShieldCheck } from 'lucide-react';
import {
  api,
  inventoryItemToSkin,
  isDevelopmentFallbackEnabled,
  type CatalogSku,
  type MarketValuation,
  type QuoteResponse,
} from '../api';
import type { ApiStatus } from '../components/AppShell';
import { ContractBuilder, type CommittedContract } from '../components/ContractBuilder';
import { items as developmentItems, results as developmentResults } from '../mocks/dev-data';
import type { InventoryItem, SkinDefinition } from '../types';
import { readContractSelectionIds, writeContractSelectionIds } from './InventoryPage';

type ContractsApi = Pick<typeof api,
  | 'inventory'
  | 'catalogSkus'
  | 'marketValuations'
  | 'allocateQuote'
  | 'createQuote'
  | 'acceptQuote'
  | 'history'
>;

type ContractStage = 'allocation' | 'quote' | 'accept';

export type ContractLifecycle = {
  allocationId: string | null;
  quoteId: string | null;
  quote: QuoteResponse | null;
  idempotencyKey: string;
  stage: ContractStage;
  submittedSnapshot: readonly string[];
  historyBeforeAccept?: ReadonlySet<string>;
};

function sameSnapshot(lifecycle: ContractLifecycle, submitted: readonly string[]) {
  return lifecycle.submittedSnapshot.length === submitted.length
    && lifecycle.submittedSnapshot.every((id, index) => id === submitted[index]);
}

type ContractsPageProps = {
  setApiStatus: (status: ApiStatus) => void;
  client?: ContractsApi;
};

function catalogCandidate(sku: CatalogSku, valuation?: MarketValuation): SkinDefinition {
  const [stableWeapon, ...stableSkinParts] = sku.stable_name.split(' | ');
  return {
    id: sku.item_id,
    weapon: sku.weapon?.trim() || (stableSkinParts.length > 0 ? stableWeapon : 'CS2'),
    skin: sku.skin_name?.trim() || stableSkinParts.join(' | ') || sku.stable_name,
    wear: sku.wear_band.replace(/_/g, ' '),
    price: valuation ? valuation.price_microcredits / 1_000_000 : null,
    color: '#58d6e7',
    rarity: sku.rarity,
    image: sku.canonical_image_url || '',
    canonicalImageUrl: sku.canonical_image_url,
  };
}

/** Catalog candidates are informational. The backend quote remains authoritative. */
export function possibleResultsFromCatalog(
  selected: InventoryItem[],
  catalog: CatalogSku[],
  valuations: MarketValuation[],
): SkinDefinition[] {
  if (selected.length === 0) return [];
  const bySku = new Map(catalog.map((sku) => [sku.sku_id, sku]));
  const selectedSkus = selected.map((item) => typeof item.skuId === 'string' ? bySku.get(item.skuId) : undefined);
  if (selectedSkus.some((sku) => !sku)) return [];
  const firstRank = selectedSkus[0]!.rarity_rank;
  if (selectedSkus.some((sku) => sku!.rarity_rank !== firstRank)) return [];

  const collections = new Set(selectedSkus.map((sku) => sku!.collection_id));
  const valuationBySku = new Map(valuations.map((valuation) => [valuation.sku_id, valuation]));
  const seenCatalogItems = new Set<string>();
  return catalog
    .filter((sku) => collections.has(sku.collection_id) && sku.rarity_rank === firstRank + 1 && Boolean(sku.canonical_image_url?.trim()))
    .filter((sku) => {
      if (seenCatalogItems.has(sku.item_id)) return false;
      seenCatalogItems.add(sku.item_id);
      return true;
    })
    .map((sku) => catalogCandidate(sku, valuationBySku.get(sku.sku_id)));
}

export function ContractsPage({ setApiStatus, client = api }: ContractsPageProps) {
  const developmentFallbackEnabled = isDevelopmentFallbackEnabled();
  const [items, setItems] = useState<InventoryItem[]>(developmentFallbackEnabled ? developmentItems : []);
  const [selected, setSelected] = useState<InventoryItem[]>(developmentFallbackEnabled ? developmentItems.slice(0, 4) : []);
  const [catalog, setCatalog] = useState<CatalogSku[]>([]);
  const [valuations, setValuations] = useState<MarketValuation[]>([]);
  const [usingDevelopmentFallback, setUsingDevelopmentFallback] = useState(developmentFallbackEnabled);
  const lifecycle = useRef<ContractLifecycle | null>(null);

  useEffect(() => {
    let active = true;
    setApiStatus('connecting');
    Promise.all([client.inventory(), client.catalogSkus(), client.marketValuations()])
      .then(([inventory, catalogPage, valuationPage]) => {
        if (!active) return;
        const catalogBySku = new Map(catalogPage.items.map((row) => [row.sku_id, row]));
        const valuationBySku = new Map(valuationPage.items.map((row) => [row.sku_id, row]));
        const mapped = inventory.items.map((row) => inventoryItemToSkin(row, catalogBySku.get(row.sku_id), valuationBySku.get(row.sku_id)));
        setApiStatus('live');
        setUsingDevelopmentFallback(false);
        setItems(mapped);
        const restored = new Set(readContractSelectionIds());
        setSelected(mapped.filter((item) => restored.has(item.id) && !item.locked).slice(0, 10));
        setCatalog(catalogPage.items);
        setValuations(valuationPage.items);
      })
      .catch(() => {
        if (!active) return;
        setApiStatus('fallback');
        setCatalog([]);
        setValuations([]);
        if (developmentFallbackEnabled) {
          setUsingDevelopmentFallback(true);
          setItems(developmentItems);
          setSelected(developmentItems.slice(0, 4));
        } else {
          setUsingDevelopmentFallback(false);
          setItems([]);
          setSelected([]);
        }
      });
    return () => { active = false; };
  // The client is an injected transport wrapper; its object identity may
  // change during a parent render without changing the authenticated data.
  // Avoid replaying inventory/catalog requests for that presentation change.
  }, [developmentFallbackEnabled]);

  const possibleResults = useMemo(
    () => usingDevelopmentFallback ? developmentResults : possibleResultsFromCatalog(selected, catalog, valuations),
    [catalog, selected, usingDevelopmentFallback, valuations],
  );

  const updateSelection = (next: InventoryItem[]) => {
    setSelected(next);
    writeContractSelectionIds(next.map((item) => item.id));
  };

  const commitContract = async (chosen: readonly InventoryItem[]): Promise<CommittedContract> => {
    const submittedSnapshot = Object.freeze(chosen.map((item) => item.publicId || item.id));
    if (!lifecycle.current || !sameSnapshot(lifecycle.current, submittedSnapshot)) {
      lifecycle.current = {
        allocationId: null,
        quoteId: null,
        quote: null,
        idempotencyKey: crypto.randomUUID(),
        stage: 'allocation',
        submittedSnapshot,
      };
    }
    const operation = lifecycle.current;

    if (operation.stage === 'allocation') {
      const allocation = await client.allocateQuote();
      operation.allocationId = allocation.allocation_id;
      operation.stage = 'quote';
    }
    if (operation.stage === 'quote') {
      try {
        const quote = await client.createQuote(
          operation.allocationId!,
          [...operation.submittedSnapshot],
          operation.idempotencyKey,
        );
        operation.quoteId = quote.quote_id;
        operation.quote = quote;
        operation.stage = 'accept';
      } catch (error) {
        // Quote creation has no idempotency key on the backend. Do not replay
        // an ambiguous request against a possibly released allocation.
        lifecycle.current = null;
        throw error;
      }
    }

    const committed = await client.acceptQuote(operation.quoteId!, operation.idempotencyKey);
    let result: InventoryItem | null = null;
    try {
      const refreshed = await client.inventory();
      const catalogBySku = new Map(catalog.map((row) => [row.sku_id, row]));
      const valuationBySku = new Map(valuations.map((row) => [row.sku_id, row]));
      const mapped = refreshed.items.map((row) => inventoryItemToSkin(row, catalogBySku.get(row.sku_id), valuationBySku.get(row.sku_id)));
      setItems(mapped);
      const outcomeIds = new Set(operation.quote?.outcomes.map((outcome) => outcome.item_id) ?? []);
      result = mapped.find((item) => outcomeIds.has(item.publicId || item.id)) ?? null;
    } catch {
      // Acceptance is already authoritative. A failed presentation refresh must
      // not replace it with a possible outcome or turn it into a fake failure.
    }
    const resultOutcome = result
      ? operation.quote?.outcomes.find((outcome) => outcome.item_id === (result.publicId || result.id))
      : undefined;
    lifecycle.current = null;
    return {
      contractId: committed.contract_id,
      result,
      inputValueMicrocredits: operation.quote?.input_value_microcredits ?? null,
      resultValueMicrocredits: resultOutcome?.buyback_microcredits ?? null,
    };
  };

  return (
    <>
      <section className="intro contracts-intro">
        <div><span className="eyebrow">КОНТРАКТЫ / WORKSPACE</span><h1>СОЗДАТЬ <em>КОНТРАКТ</em></h1><p>Выберите от 4 до 10 скинов. Сервер зафиксирует состав, подпишет контракт и вернёт подтверждённый ID.</p></div>
        <div className="trust"><ShieldCheck size={17} /> СЕРВЕРНОЕ ПОДТВЕРЖДЕНИЕ <span>·</span> CC ECONOMY</div>
      </section>
      <ol className="contract-flow" aria-label="Этапы контракта">
        {['SELECT', 'LOCK', 'SIGN', 'MERGE', 'RESULT'].map((step, index) => <li className={index === 0 ? 'active' : ''} key={step}><span>{String(index + 1).padStart(2, '0')}</span>{step}</li>)}
      </ol>
      {usingDevelopmentFallback && <p className="development-data-notice" role="status">Показаны явно включённые данные разработки. Они не являются подтверждённой операцией.</p>}
      <ContractBuilder
        items={items}
        selected={selected}
        onSelectionChange={updateSelection}
        possibleResults={possibleResults}
        possibleResultsAreDevelopmentData={usingDevelopmentFallback}
        onSubmit={commitContract}
      />
    </>
  );
}
