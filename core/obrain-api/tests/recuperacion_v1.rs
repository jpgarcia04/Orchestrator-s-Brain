#[path = "fixtures/legacy_v1.rs"]
mod legacy;
use legacy::*;
use obrain_api::Brain;
use obrain_core::{EntityId, Store, TimePoint};

#[test]
fn bootstrap_v1_no_publica_vencimiento_obsoleto() {
    let legacy = Legacy::new("stale-expiry", &[doc("Documento normal sin fecha.")]);
    let migrated = Store::open(legacy.path()).unwrap();
    assert!(migrated.content_pending(&EntityId(DOC.into())).unwrap());
    assert_eq!(migrated.log_len().unwrap(), 9);
    drop(migrated);
    let brain = Brain::open(legacy.path()).unwrap();
    let id = EntityId(DOC.into());
    assert!(!brain.store().content_pending(&id).unwrap());
    assert!(
        brain
            .store()
            .resolve(&id, "expires_on", TimePoint::at(obrain_scan::now_ms()))
            .unwrap()
            .is_none()
    );
    assert!(
        brain
            .preview(&id, None)
            .unwrap()
            .text
            .unwrap()
            .contains("normal")
    );
    assert_eq!(brain.search("Documento", 10).unwrap().hits.len(), 1);
    assert_eq!(brain.store().sensitivity(&id).unwrap().as_str(), "normal");
    let log = brain.store().log_len().unwrap();
    drop(brain);
    let reopened = Brain::open(legacy.path()).unwrap();
    assert_eq!(reopened.store().log_len().unwrap(), log);
}
