#[path = "fixtures/legacy_v1.rs"]
mod legacy;
use legacy::*;
use obrain_api::Brain;
use obrain_core::{EntityId, Store, TimePoint};

#[test]
fn bootstrap_v1_fallido_se_revierte_y_reintenta() {
    let legacy = Legacy::new("bootstrap-fail", &[doc("Documento normal sin fecha.")]);
    let migrated = Store::open(legacy.path()).unwrap();
    let log = migrated.log_len().unwrap();
    migrated
        .conn()
        .execute_batch(
            "CREATE TRIGGER abort_expiry_v1
             BEFORE UPDATE OF retracted_at ON assertions
             WHEN OLD.predicate='expires_on' AND OLD.source LIKE 'expiry%'
             BEGIN SELECT RAISE(ABORT, 'fallo sintético en bootstrap'); END",
        )
        .unwrap();
    drop(migrated);
    assert!(Brain::try_from_store(Store::open(legacy.path()).unwrap()).is_err());
    let pending = Store::open(legacy.path()).unwrap();
    assert!(pending.content_pending(&EntityId(DOC.into())).unwrap());
    assert_eq!(pending.log_len().unwrap(), log);
    assert_eq!(
        pending.sensitivity(&EntityId(DOC.into())).unwrap().as_str(),
        "vault"
    );
    pending
        .conn()
        .execute_batch("DROP TRIGGER abort_expiry_v1")
        .unwrap();
    drop(pending);
    let recovered = Brain::try_from_store(Store::open(legacy.path()).unwrap()).unwrap();
    assert!(
        !recovered
            .store()
            .content_pending(&EntityId(DOC.into()))
            .unwrap()
    );
    assert!(
        recovered
            .store()
            .resolve(
                &EntityId(DOC.into()),
                "expires_on",
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .is_none()
    );
}
