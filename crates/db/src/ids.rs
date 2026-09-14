use uuid::Uuid;

macro_rules! internal_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, sqlx::Type)]
        #[sqlx(transparent)]
        pub struct $name(i64);

        impl $name {
            pub const fn new(value: i64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> i64 {
                self.0
            }
        }
    };
}

internal_id!(UserId);
internal_id!(AdministratorId);
internal_id!(CriticalActionId);
internal_id!(CollectionId);
internal_id!(CatalogItemId);
internal_id!(SkuId);
internal_id!(InventoryItemId);
internal_id!(LedgerAccountId);
internal_id!(LedgerTransactionId);
internal_id!(CreditAdjustmentEventId);
internal_id!(ValuationSnapshotId);
internal_id!(QuoteId);
internal_id!(ContractId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct PublicId(Uuid);

impl PublicId {
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn get(self) -> Uuid {
        self.0
    }
}
