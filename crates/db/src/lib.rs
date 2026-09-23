mod catalog;
mod config;
mod contracts;
mod database;
mod identity;
mod ids;
mod inventory;
mod ledger;
mod market_orders;
mod pricing;
mod quotes;
mod scarcity;
mod stock;

pub use catalog::{
    CatalogCollection, CatalogItem, CatalogSku, Collection, Sku, find_active_skus,
    find_catalog_item_by_public_id, find_collection_by_public_id, find_sku_by_public_id,
    list_catalog_skus, list_catalog_skus_after, list_collections,
};
pub use config::{DatabaseConfig, DatabaseConfigError};
pub use contracts::{
    Contract, ContractInput, ContractOutcome, finalize_contract_for_user,
    find_contract_by_public_id, find_contract_by_quote_id, find_contract_outcome,
    list_contract_inputs,
};
pub use database::{Database, DatabaseError, MIGRATOR};
pub use identity::{
    Account, ActiveSession, UserCredential, create_user_session, find_account_by_public_id,
    find_active_user_session, find_user_credential_by_login, find_user_credit_balance,
    register_invited_user, revoke_all_user_sessions, revoke_user_session,
};
pub use ids::*;
pub use inventory::{
    InventoryItem, OwnedInventoryItem, find_inventory_item, find_owned_inventory_item,
    list_available_user_inventory, list_available_warehouse_inventory, list_owned_inventory,
};
pub use ledger::{
    CreditAdjustmentEvent, LedgerAccount, LedgerBalance, LedgerBalanceDrift,
    find_credit_adjustment, find_ledger_account, find_ledger_balance, post_credit_adjustment,
    reconcile_ledger_balances,
};
pub use market_orders::{MarketOrderResult, buyback_market_item, purchase_market_item};
pub use pricing::{
    CurrentValuation, CurrentValuationDrift, MarketPriceHalt, MarketValuation, PriceHalt,
    PublishedValuationSnapshotDrift, SnapshotValuation, ValuationSnapshot, find_current_valuation,
    find_valuation_snapshot, list_active_price_halts, list_market_price_halts_after,
    list_market_valuations_after, list_snapshot_valuations, list_tradeable_current_valuations,
    reconcile_current_valuations, reconcile_published_valuation_snapshots,
};
pub use quotes::{
    QuoteInput, QuoteOutcome, SeedAllocationRequest, TradeupQuote, allocate_seed_for_user,
    find_active_quote_for_user, find_tradeup_quote, list_quote_inputs, list_quote_outcomes,
};
pub use scarcity::{
    CollectionScarcity, CollectionScarcityDrift, CollectionScarcitySnapshotItem,
    find_current_collection_scarcity, list_collection_scarcity_history,
    publish_collection_scarcity_snapshot, reconcile_current_collection_scarcity,
};
/// Re-exported so callers can name the timestamp type in this crate's
/// row structs without taking their own dependency on SQLx. `db` is the
/// PostgreSQL boundary; nothing above it should need to link the driver
/// just to spell `DateTime<Utc>`.
pub use sqlx::types::chrono;
pub use sqlx::{PgPool, Postgres, Transaction};
pub use stock::{
    RiskState, StockPolicyBand, StockPolicyVersion, WarehouseStock, find_active_stock_policy_bands,
    find_active_stock_policy_version, find_risk_state, find_warehouse_stock,
};
