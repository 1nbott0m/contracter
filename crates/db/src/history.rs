use crate::chrono::{DateTime, Utc};
use crate::{DatabaseError, UserId};
use sqlx::{Executor, Postgres};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContractHistoryRow {
    pub public_id: uuid::Uuid,
    pub status_code: String,
    pub created_at: DateTime<Utc>,
    pub ledger_transaction_id: Option<i64>,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LedgerHistoryRow {
    pub public_id: uuid::Uuid,
    pub operation_kind: String,
    pub amount_microcredits: i64,
    pub occurred_at: DateTime<Utc>,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InventoryEventHistoryRow {
    pub public_id: uuid::Uuid,
    pub inventory_item_id: uuid::Uuid,
    pub event_kind_code: String,
    pub operation_public_id: uuid::Uuid,
    pub occurred_at: DateTime<Utc>,
}

pub async fn list_contract_history<'e, E>(
    executor: E,
    user_id: UserId,
    before: Option<uuid::Uuid>,
    limit: i64,
) -> Result<Vec<ContractHistoryRow>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT c.public_id, c.status_code, c.created_at, c.ledger_transaction_id FROM contracts c WHERE c.user_id=$1 AND ($2::uuid IS NULL OR c.public_id < $2) ORDER BY c.public_id DESC LIMIT $3").bind(user_id).bind(before).bind(limit).fetch_all(executor).await?)
}
pub async fn list_ledger_history<'e, E>(
    executor: E,
    user_id: UserId,
    before: Option<uuid::Uuid>,
    limit: i64,
) -> Result<Vec<LedgerHistoryRow>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT t.public_id, t.operation_kind, p.amount_microcredits, t.created_at AS occurred_at FROM ledger_transactions t JOIN ledger_postings p ON p.ledger_transaction_id=t.id JOIN ledger_accounts a ON a.id=p.account_id WHERE a.owner_user_id=$1 AND ($2::uuid IS NULL OR t.public_id < $2) ORDER BY t.public_id DESC LIMIT $3").bind(user_id).bind(before).bind(limit).fetch_all(executor).await?)
}
pub async fn list_inventory_event_history<'e, E>(
    executor: E,
    user_id: UserId,
    before: Option<uuid::Uuid>,
    limit: i64,
) -> Result<Vec<InventoryEventHistoryRow>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT e.public_id, i.public_id AS inventory_item_id, e.event_kind_code, e.operation_public_id, e.occurred_at FROM inventory_transfer_events e JOIN inventory_items i ON i.id=e.inventory_item_id WHERE (e.to_user_id=$1 OR e.from_user_id=$1) AND ($2::uuid IS NULL OR e.public_id < $2) ORDER BY e.public_id DESC LIMIT $3").bind(user_id).bind(before).bind(limit).fetch_all(executor).await?)
}
