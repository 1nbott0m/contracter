use sqlx::{
    Executor, PgConnection, Postgres,
    types::chrono::{DateTime, Utc},
};
use uuid::Uuid;

use crate::{
    AdministratorId, CreditAdjustmentEventId, CriticalActionId, DatabaseError, LedgerAccountId,
    LedgerTransactionId, PublicId, UserId,
};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LedgerAccount {
    pub id: LedgerAccountId,
    pub public_id: PublicId,
    pub kind_code: String,
    pub owner_user_id: Option<UserId>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LedgerBalance {
    pub account_id: LedgerAccountId,
    pub balance_microcredits: i64,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CreditAdjustmentEvent {
    pub id: CreditAdjustmentEventId,
    pub public_id: PublicId,
    pub initiator_admin_id: AdministratorId,
    pub target_user_id: UserId,
    pub amount_microcredits: i64,
    pub execution_key: Uuid,
    pub critical_action_id: Option<CriticalActionId>,
    pub ledger_transaction_id: LedgerTransactionId,
    pub occurred_at: DateTime<Utc>,
}

pub async fn find_ledger_account<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<LedgerAccount>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, kind_code, owner_user_id, created_at, closed_at \
         FROM ledger_accounts WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_ledger_balance<'e, E>(
    executor: E,
    account_id: LedgerAccountId,
) -> Result<Option<LedgerBalance>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT account_id, balance_microcredits, version, updated_at \
         FROM ledger_balances WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(executor)
    .await?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct LedgerBalanceDrift {
    pub account_id: LedgerAccountId,
    pub cached_balance_microcredits: i64,
    pub posted_balance_microcredits: i64,
}

/// Diagnostic-only: rows where `ledger_balances`'s cached balance disagrees
/// with the sum of that account's append-only `ledger_postings`. An empty
/// result is the expected, healthy state; a non-empty one is drift to
/// investigate, not something this function can itself repair.
pub async fn reconcile_ledger_balances<'e, E>(
    executor: E,
) -> Result<Vec<LedgerBalanceDrift>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT * FROM reconcile_ledger_balances()")
        .fetch_all(executor)
        .await?)
}

pub async fn find_credit_adjustment<'e, E>(
    executor: E,
    execution_key: Uuid,
) -> Result<Option<CreditAdjustmentEvent>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, initiator_admin_id, target_user_id, \
                amount_microcredits, execution_key, critical_action_id, \
                ledger_transaction_id, occurred_at \
         FROM credit_adjustment_events WHERE execution_key = $1",
    )
    .bind(execution_key)
    .fetch_optional(executor)
    .await?)
}

pub async fn post_credit_adjustment(
    connection: &mut PgConnection,
    initiator_admin_id: AdministratorId,
    target_user_id: UserId,
    amount_microcredits: i64,
    execution_key: Uuid,
    critical_action_id: Option<CriticalActionId>,
) -> Result<LedgerTransactionId, DatabaseError> {
    Ok(
        sqlx::query_scalar("SELECT post_credit_adjustment($1, $2, $3, $4, $5)")
            .bind(initiator_admin_id)
            .bind(target_user_id)
            .bind(amount_microcredits)
            .bind(execution_key)
            .bind(critical_action_id)
            .fetch_one(connection)
            .await?,
    )
}
