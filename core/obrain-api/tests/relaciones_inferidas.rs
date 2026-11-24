//! Regresión de `version_of` y `occurred_during` en el recorrido real de
//! indexación: una relación inferida que deja de cumplirse se retracta, en el
//! log y en la ficha, y la historia sigue diciendo que el sistema la creyó.
//! Sólo bibliotecas sintéticas en la carpeta temporal.

use obrain_api::{Brain, indexar};
use obrain_core::{EntityId, Store, TimePoint};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

struct Library(PathBuf);

impl Library {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "obrain-relaciones-inferidas-{}-{}",
            std::process::id(),
            EntityId::new()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn file(&self, name: &str, content: &str) {
        std::fs::write(self.0.join(name), content).unwrap();
    }

    /// Fija el `mtime`, que es todo lo que mira el enlazador de sesiones.
    fn touch(&self, name: &str, ms: u64) {
        std::fs::File::options()
            .write(true)
            .open(self.0.join(name))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_millis(ms))
            .unwrap();
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

fn activas(store: &Store, predicate: &str) -> Vec<obrain_core::Assertion> {
    store
        .query(None, Some(predicate), TimePoint::at(obrain_scan::now_ms()))
        .unwrap()
}

fn en_la_ficha(brain: &Brain, id: &EntityId, predicate: &str) -> bool {
    brain
        .entity(id)
        .unwrap()
        .inferred
        .iter()
        .any(|c| c.predicate == predicate)
}

#[test]
fn renombrar_una_version_deshace_la_cadena_en_log_y_ficha() {
    let library = Library::new();
    library.file("informe.txt", "Primer borrador sintético de un informe.");
    library.file(
        "informe_v2.txt",
        "Segundo borrador sintético, ya corregido.",
    );
    library.touch("informe.txt", 1_700_000_000_000);
    library.touch("informe_v2.txt", 1_700_100_000_000);
    let mut store = Store::open_in_memory().unwrap();
    index(&mut store, &library);
    let viejo = library.id(&store, "informe.txt");
    let original = activas(&store, "version_of");
    assert_eq!(original.len(), 1);
    assert_eq!(original[0].subject, viejo);

    std::thread::sleep(Duration::from_millis(20));
    std::fs::rename(
        library.path().join("informe_v2.txt"),
        library.path().join("presentacion.txt"),
    )
    .unwrap();
    index(&mut store, &library);
    assert!(activas(&store, "version_of").is_empty());
    assert_eq!(
        store
            .query(
                None,
                Some("version_of"),
                TimePoint::at(original[0].asserted_at)
            )
            .unwrap()
            .len(),
        1,
        "la historia conserva lo que el sistema creyó"
    );

    let brain = Brain::from_store(store);
    assert!(!en_la_ficha(&brain, &viejo, "version_of"));
}

#[test]
fn editar_un_archivo_otro_dia_lo_saca_de_su_sesion_en_log_y_ficha() {
    let library = Library::new();
    let tarde: u64 = 1_700_000_000_000;
    for (i, nombre) in ["a.txt", "b.txt", "c.txt"].iter().enumerate() {
        library.file(nombre, &format!("Nota sintética número {i} de una tarde."));
        library.touch(nombre, tarde + i as u64 * 5 * 60 * 1000);
    }
    let mut store = Store::open_in_memory().unwrap();
    index(&mut store, &library);
    let original = activas(&store, "occurred_during");
    assert_eq!(original.len(), 3);

    std::thread::sleep(Duration::from_millis(20));
    library.touch("c.txt", tarde + 3 * 24 * 60 * 60 * 1000);
    index(&mut store, &library);
    assert!(
        activas(&store, "occurred_during").is_empty(),
        "dos archivos de aquella tarde ya no son una sesión"
    );
    assert_eq!(
        store
            .query(
                None,
                Some("occurred_during"),
                TimePoint::at(original[0].asserted_at)
            )
            .unwrap()
            .len(),
        3
    );

    let a = library.id(&store, "a.txt");
    let brain = Brain::from_store(store);
    assert!(!en_la_ficha(&brain, &a, "occurred_during"));
}
