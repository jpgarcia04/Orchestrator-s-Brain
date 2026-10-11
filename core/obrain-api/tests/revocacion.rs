//! Pipeline completo, sin usar la bóveda ni pesos de un modelo real.
use obrain_api::indexar;
use obrain_core::{Sensitivity, Store, TimePoint};
use obrain_embed::embedder::{Embedder, ModelId, Role};

struct Modelo;
impl Embedder for Modelo {
    fn model(&self) -> ModelId {
        ModelId {
            id: "fixture",
            version: 1,
            dim: 2,
        }
    }
    fn embed(&self, texts: &[&str], _: Role) -> obrain_embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }
    fn batch_size(&self) -> usize {
        2
    }
}

#[test]
fn reindexar_sin_modelo_revoca_embeddings_existentes_al_reclasificar() {
    let root = std::env::temp_dir().join(format!(
        "obrain-revocacion-{}",
        obrain_core::EntityId::new()
    ));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(
        root.join("documento.txt"),
        "CONTENIDO SINTETICO para una prueba aislada.",
    )
    .unwrap();
    let mut store = Store::open_in_memory().unwrap();
    let first =
        indexar::index_with(&mut store, std::slice::from_ref(&root), Some(&Modelo), None).unwrap();
    assert!(first.vectors_new > 0);
    let before = obrain_scan::now_ms();
    let files = obrain_extract::load_files(&store, TimePoint::at(before)).unwrap();
    assert_eq!(files.len(), 1);
    let e = &files[0].entity;
    let old = store
        .resolve(e, "embedded_with", TimePoint::at(before))
        .unwrap()
        .unwrap();
    std::fs::rename(root.join("documento.txt"), root.join("credentials.txt")).unwrap();
    while obrain_scan::now_ms() <= before {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // None salta embed_all: la revocación debe ocurrir al clasificar.
    let second = indexar::index_with(&mut store, std::slice::from_ref(&root), None, None).unwrap();
    assert!(!second.semantic);
    assert_eq!(
        second.documents_read, 0,
        "el texto no cambió: no basta el cascade de chunks"
    );
    assert_eq!(store.sensitivity(e).unwrap(), Sensitivity::Vault);
    assert_eq!(
        store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM vectors WHERE entity = ?1",
                [e.as_str()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    for p in ["embedded_with", "vector_count"] {
        assert!(
            store
                .resolve(e, p, TimePoint::at(obrain_scan::now_ms()))
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(
        store
            .resolve(e, "embedded_with", TimePoint::at(before))
            .unwrap()
            .unwrap()
            .id,
        old.id
    );
}
