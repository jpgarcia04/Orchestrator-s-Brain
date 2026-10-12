#[path = "fixtures/legacy_v1.rs"]
mod legacy;
use legacy::*;
use obrain_api::Brain;
use obrain_core::{EntityId, Method, Store, TimePoint};

#[test]
fn bootstrap_v1_preserva_correccion_y_no_publica_extraccion_inverificable() {
    let mut corrected = doc("Documento normal sin fecha.");
    corrected.manual_date = Some("2025-12-01");
    corrected.manual_sensitivity = Some("sensitive");
    let mut invalid = doc("Contenido no verificable.");
    invalid.id = "legacy-unverified";
    invalid.name = "inverificable.txt";
    invalid.extraction_verified = false;
    let legacy = Legacy::new("mixed", &[corrected, invalid]);
    let brain = Brain::try_from_store(Store::open(legacy.path()).unwrap()).unwrap();
    let corrected_id = EntityId(DOC.into());
    let invalid_id = EntityId("legacy-unverified".into());
    assert!(!brain.store().content_pending(&corrected_id).unwrap());
    assert!(brain.store().content_pending(&invalid_id).unwrap());
    let dates = brain
        .store()
        .query(
            Some(&corrected_id),
            Some("expires_on"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].method, Method::User);
    assert_eq!(dates[0].object.as_text(), Some("2025-12-01"));
    assert_eq!(
        brain.store().sensitivity(&corrected_id).unwrap().as_str(),
        "sensitive"
    );
    assert_eq!(
        obrain_extract::index::document_text(brain.store().conn(), &invalid_id)
            .unwrap()
            .as_deref(),
        Some("Contenido no verificable.")
    );
    assert!(brain.preview(&invalid_id, None).is_err());
    assert!(brain.search("inverificable", 10).unwrap().hits.is_empty());
    assert!(
        brain
            .expiring()
            .unwrap()
            .iter()
            .all(|e| e.row.id != invalid_id.0)
    );
    assert!(
        brain
            .store()
            .resolve(
                &invalid_id,
                "expires_on",
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .is_some()
    );
}
