use db::{ContractId, PublicId, SkuId, UserId};
use uuid::Uuid;

#[test]
fn typed_internal_ids_keep_their_values() {
    assert_eq!(UserId::new(7).get(), 7);
    assert_eq!(SkuId::new(7).get(), 7);
    assert_eq!(ContractId::new(11).get(), 11);
}

#[test]
fn public_id_wraps_uuid_without_stringly_typed_boundaries() {
    let uuid = Uuid::new_v4();
    assert_eq!(PublicId::new(uuid).get(), uuid);
}
