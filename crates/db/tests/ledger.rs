use db::{
    AdministratorId, CriticalActionId, Database, DatabaseConfig, LedgerAccountId, PublicId, UserId,
    find_credit_adjustment, find_ledger_account, find_ledger_balance, post_credit_adjustment,
};
use sqlx::{Executor, Postgres, Transaction};
use uuid::Uuid;

async fn test_database() -> Database {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    database
}

async fn isolated_transaction(database: &Database) -> Transaction<'_, Postgres> {
    let mut transaction = database.begin().await.expect("begin transaction");
    transaction
        .execute("SELECT pg_advisory_xact_lock(812_202_609_14)")
        .await
        .expect("serialize ledger integration fixtures");
    transaction
}

async fn seed_credit_adjustment_fixture(
    transaction: &mut Transaction<'_, Postgres>,
) -> (AdministratorId, UserId, LedgerAccountId, PublicId) {
    transaction
        .execute(
            "INSERT INTO ledger_account_kinds (code, description) VALUES \
             ('system_treasury', 'test treasury'), ('user_credit', 'test user credit') \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed ledger account kinds");

    let suffix = Uuid::new_v4().simple().to_string();
    let admin_public_id = PublicId::new(Uuid::new_v4());
    let target_public_id = PublicId::new(Uuid::new_v4());
    let target_account_public_id = PublicId::new(Uuid::new_v4());

    let admin_user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (public_id, login, password_hash) \
         VALUES ($1, $2, 'argon2id-test-hash') RETURNING id",
    )
    .bind(admin_public_id)
    .bind(format!("ledger_admin_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert admin user");

    let target_user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (public_id, login, password_hash) \
         VALUES ($1, $2, 'argon2id-test-hash') RETURNING id",
    )
    .bind(target_public_id)
    .bind(format!("ledger_target_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert target user");

    let administrator_id: AdministratorId =
        sqlx::query_scalar("INSERT INTO administrators (user_id) VALUES ($1) RETURNING id")
            .bind(admin_user_id)
            .fetch_one(transaction.as_mut())
            .await
            .expect("insert administrator");

    sqlx::query(
        "INSERT INTO ledger_accounts (kind_code) VALUES ('system_treasury') \
         ON CONFLICT DO NOTHING",
    )
    .execute(transaction.as_mut())
    .await
    .expect("ensure system treasury");

    let target_account_id: LedgerAccountId = sqlx::query_scalar(
        "INSERT INTO ledger_accounts (public_id, kind_code, owner_user_id) \
         VALUES ($1, 'user_credit', $2) RETURNING id",
    )
    .bind(target_account_public_id)
    .bind(target_user_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert target ledger account");

    (
        administrator_id,
        target_user_id,
        target_account_id,
        target_account_public_id,
    )
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn credit_adjustment_updates_balance_and_is_idempotent() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (administrator_id, target_user_id, account_id, account_public_id) =
        seed_credit_adjustment_fixture(&mut transaction).await;
    let execution_key = Uuid::new_v4();

    let first_id = post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        25_000_000,
        execution_key,
        None,
    )
    .await
    .expect("post credit adjustment");
    let repeated_id = post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        25_000_000,
        execution_key,
        None,
    )
    .await
    .expect("repeat credit adjustment");

    assert_eq!(first_id, repeated_id);
    transaction
        .execute("SAVEPOINT conflicting_adjustment")
        .await
        .expect("create conflict savepoint");
    let conflict = post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        30_000_000,
        execution_key,
        None,
    )
    .await
    .expect_err("same execution key with another request must fail");
    assert_eq!(conflict.database_code().as_deref(), Some("23505"));
    transaction
        .execute("ROLLBACK TO SAVEPOINT conflicting_adjustment")
        .await
        .expect("recover after expected idempotency conflict");

    assert_eq!(
        find_ledger_account(transaction.as_mut(), account_public_id)
            .await
            .expect("read ledger account")
            .expect("ledger account exists")
            .id,
        account_id
    );
    let balance = find_ledger_balance(transaction.as_mut(), account_id)
        .await
        .expect("read ledger balance")
        .expect("ledger balance exists");
    assert_eq!(balance.balance_microcredits, 25_000_000);
    assert_eq!(balance.version, 1);

    let event = find_credit_adjustment(transaction.as_mut(), execution_key)
        .await
        .expect("read credit adjustment")
        .expect("credit adjustment exists");
    assert_eq!(event.ledger_transaction_id, first_id);
    assert_eq!(event.amount_microcredits, 25_000_000);
    assert_eq!(event.critical_action_id, None::<CriticalActionId>);

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn user_credit_balance_cannot_become_negative() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (administrator_id, target_user_id, _, _) =
        seed_credit_adjustment_fixture(&mut transaction).await;
    let error = post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        -1,
        Uuid::new_v4(),
        None,
    )
    .await
    .expect_err("a user credit account must not settle below zero");
    assert_eq!(error.database_code().as_deref(), Some("23514"));
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn invalid_adjustment_rolls_back_without_creating_an_event_or_balance() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (administrator_id, target_user_id, account_id, _) =
        seed_credit_adjustment_fixture(&mut transaction).await;
    let execution_key = Uuid::new_v4();
    transaction
        .execute("SAVEPOINT rejected_adjustment")
        .await
        .expect("create savepoint");

    let error = post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        0,
        execution_key,
        None,
    )
    .await
    .expect_err("zero adjustment must fail");

    assert_eq!(error.database_code().as_deref(), Some("23514"));
    transaction
        .execute("ROLLBACK TO SAVEPOINT rejected_adjustment")
        .await
        .expect("recover after expected PostgreSQL error");
    assert!(
        find_credit_adjustment(transaction.as_mut(), execution_key)
            .await
            .expect("read absent event")
            .is_none()
    );
    assert!(
        find_ledger_balance(transaction.as_mut(), account_id)
            .await
            .expect("read absent balance")
            .is_none()
    );
    assert!(
        find_ledger_account(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("read unknown ledger account")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn public_role_cannot_call_privileged_ledger_writers_or_mutate_journals() {
    let database = test_database().await;

    let boundary_is_closed: bool = sqlx::query_scalar(
        "SELECT NOT has_function_privilege( \
                    'public', \
                    'public.post_ledger_transaction(text,uuid,jsonb)', \
                    'EXECUTE' \
                ) \
                AND NOT has_function_privilege( \
                    'public', \
                    'public.post_credit_adjustment(bigint,bigint,bigint,uuid,bigint)', \
                    'EXECUTE' \
                ) \
                AND ( \
                    SELECT bool_and( \
                        NOT has_table_privilege( \
                            'public', protected_table.name, mutation.name \
                        ) \
                    ) \
                    FROM (VALUES \
                        ('public.ledger_transactions'), \
                        ('public.ledger_postings'), \
                        ('public.credit_adjustment_events'), \
                        ('public.ledger_balances') \
                    ) AS protected_table(name) \
                    CROSS JOIN (VALUES \
                        ('INSERT'), ('UPDATE'), ('DELETE'), ('TRUNCATE') \
                    ) AS mutation(name) \
                )",
    )
    .fetch_one(database.pool())
    .await
    .expect("inspect public ledger privileges");

    assert!(boundary_is_closed);
}
