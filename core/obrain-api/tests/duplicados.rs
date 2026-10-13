//! Regresión de relaciones por hash en el recorrido real de indexación.

use obrain_api::{Brain, indexar};
use obrain_core::{EntityId, Store, TimePoint};
use std::path::{Path, PathBuf};

struct Library(PathBuf);

impl Library {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "obrain-duplicados-{}-{}",
            std::process::id(),
            EntityId::new()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn file(&self, name: &str, content: &str) {
        std::fs::write(self.0.join(name), content).unwrap();
    }

    fn id(&self, store: &Store, name: &str) -> EntityId {
        store
            .find_subject_by_value(
                "path_observed_at",
                &self.0.join(name).to_string_lossy(),
                TimePoint::at(obrain_scan::now_ms()),
            )
            .unwrap()
            .unwrap()
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn index(store: &mut Store, library: &Library) {
    let report = indexar::index_with(store, &[library.path().to_path_buf()], None, None).unwrap();
    assert!(
        report.errors.is_empty() && report.deferred.is_empty(),
        "{report:?}"
    );
}

#[test]
fn una_copia_modificada_deja_de_ser_duplicada_en_log_y_producto() {
    let library = Library::new();
    library.file(
        "a.txt",
        "Contenido sintético compartido, con suficiente texto para extraer.",
    );
    library.file(
        "b.txt",
        "Contenido sintético compartido, con suficiente texto para extraer.",
    );
    let mut store = Store::open_in_memory().unwrap();
    index(&mut store, &library);
    let a = library.id(&store, "a.txt");
    let b = library.id(&store, "b.txt");
    let original = store
        .query(
            None,
            Some("duplicate_of"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(original.len(), 1);
    assert!(matches!(original[0].object.as_entity(), Some(e) if e == &a || e == &b));

    std::thread::sleep(std::time::Duration::from_millis(20));
    library.file(
        "b.txt",
        "Contenido sintético distinto, con suficiente texto para extraer.",
    );
    index(&mut store, &library);
    let now = TimePoint::at(obrain_scan::now_ms());
    assert!(
        store
            .query(None, Some("duplicate_of"), now)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .query(
                None,
                Some("duplicate_of"),
                TimePoint::at(original[0].asserted_at)
            )
            .unwrap()
            .len(),
        1
    );

    let brain = Brain::from_store(store);
    for id in [&a, &b] {
        assert!(
            !brain
                .entity(id)
                .unwrap()
                .observed
                .iter()
                .any(|c| c.predicate == "duplicate_of")
        );
        assert!(
            !brain
                .lens(std::slice::from_ref(id), 1, 30)
                .unwrap()
                .edges
                .iter()
                .any(|e| e.predicate == "duplicate_of")
        );
    }
}

#[test]
fn una_copia_borrada_y_restaurada_recupera_su_relacion_sin_duplicarla() {
    let library = Library::new();
    let content = "Documento sintético idéntico en dos archivos de esta biblioteca.";
    library.file("a.txt", content);
    library.file("b.txt", content);
    let mut store = Store::open_in_memory().unwrap();
    index(&mut store, &library);
    let a = library.id(&store, "a.txt");
    let b = library.id(&store, "b.txt");
    let original = store
        .query(
            None,
            Some("duplicate_of"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(original.len(), 1);
    assert_eq!(original[0].subject, b);

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::remove_file(library.path().join("b.txt")).unwrap();
    index(&mut store, &library);
    assert!(
        store
            .query(
                None,
                Some("duplicate_of"),
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .is_empty()
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    library.file("b.txt", content);
    index(&mut store, &library);
    let restored = library.id(&store, "b.txt");
    index(&mut store, &library);
    let active = store
        .query(
            None,
            Some("duplicate_of"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].subject, restored);
    assert_eq!(active[0].object.as_entity(), Some(&a));
    assert_eq!(
        store
            .query(
                None,
                Some("duplicate_of"),
                TimePoint::at(original[0].asserted_at)
            )
            .unwrap()
            .len(),
        1
    );
    let brain = Brain::from_store(store);
    assert!(
        brain
            .entity(&restored)
            .unwrap()
            .observed
            .iter()
            .any(|c| c.predicate == "duplicate_of")
    );
    assert_eq!(
        brain
            .lens(&[a], 1, 30)
            .unwrap()
            .edges
            .iter()
            .filter(|e| e.predicate == "duplicate_of")
            .count(),
        1
    );
}
