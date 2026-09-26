import type { CatalogSku, MarketValuation } from './api';
import type { InventoryItem } from './types';

const MAX_INPUT_VALUE_MICROCREDITS = 15_000_000_000;
const COVERT_RARITY_RANK = 5;

export type ContractEligibility = {
  item: InventoryItem;
  reason: string | null;
};

/** Mirrors the server's trade-up prerequisites for the inventory picker.
 * The server remains authoritative; this only prevents obviously impossible
 * choices from being presented as actionable UI. */
export function eligibleContractItems(
  items: InventoryItem[],
  catalog: CatalogSku[],
  valuations: MarketValuation[],
  selected: InventoryItem[] = [],
): ContractEligibility[] {
  const bySku = new Map(catalog.map((row) => [row.sku_id, row]));
  const valuationBySku = new Map(valuations.map((row) => [row.sku_id, row]));
  const selectedSkus = selected.map((entry) => entry.skuId !== undefined ? bySku.get(String(entry.skuId)) : undefined).filter(Boolean) as CatalogSku[];
  const selectedIds = new Set(selected.map((entry) => entry.id));
  const selectedRank = selectedSkus[0]?.rarity_rank;
  const selectedValue = selected.reduce((sum, entry) => {
    const valuation = entry.skuId !== undefined ? valuationBySku.get(String(entry.skuId)) : undefined;
    return sum + (valuation?.price_microcredits ?? 0);
  }, 0);

  return items.map((item) => {
    const sku = item.skuId !== undefined ? bySku.get(String(item.skuId)) : undefined;
    const valuation = item.skuId !== undefined ? valuationBySku.get(String(item.skuId)) : undefined;
    let reason: string | null = null;
    if (item.locked) reason = 'Предмет уже используется в операции';
    else if (!sku || !valuation || !valuation.available) reason = 'Нет подтверждённой оценки';
    else if (sku.rarity_rank >= COVERT_RARITY_RANK) reason = 'Эта редкость не участвует в контрактах';
    else if (selectedRank !== undefined && sku.rarity_rank !== selectedRank) reason = 'Нужна та же редкость, что у выбранных предметов';
    else if (!selectedIds.has(item.id) && selectedValue + valuation.price_microcredits >= MAX_INPUT_VALUE_MICROCREDITS) reason = 'Сумма входа должна быть меньше 15 000 CC';
    else {
      const outputRank = sku.rarity_rank + 1;
      const outputAvailable = catalog.some((candidate) => candidate.collection_id === sku.collection_id
        && candidate.rarity_rank === outputRank
        && valuationBySku.get(candidate.sku_id)?.available === true);
      if (!outputAvailable) reason = 'Для коллекции нет доступного результата';
    }
    return { item, reason };
  }).filter((entry) => entry.reason === null);
}

export { MAX_INPUT_VALUE_MICROCREDITS };
