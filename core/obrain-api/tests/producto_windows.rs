//! Oráculo documental de la biblioteca sintética de producto.
//! Usa el pipeline real, sin modelo ni interfaz; no acredita el instalador.

use obrain_api::{Brain, indexar};
use obrain_core::{EntityId, Sensitivity, Store, TimePoint};
use std::path::{Path, PathBuf};

struct Biblioteca(PathBuf);

impl Biblioteca {
    fn nueva() -> Self {
        let root = std::env::temp_dir().join(format!(
            "obrain-producto-{}-{}",
            std::process::id(),
            EntityId::new()
        ));
        let fuente =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/producto-windows");
        copiar(&fuente, &root);
        Self(root)
    }

    fn ruta(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.0.clone(), |p, s| p.join(s))
    }

    fn id(&self, store: &Store, rel: &str) -> EntityId {
        store
            .find_subject_by_value(
                "path_observed_at",
                &self.ruta(rel).to_string_lossy(),
                TimePoint::at(obrain_scan::now_ms()),
            )
            .unwrap()
            .unwrap_or_else(|| panic!("falta {rel}"))
    }

    fn indexar(&self, store: &mut Store) {
        let r = indexar::index_with(store, std::slice::from_ref(&self.0), None, None).unwrap();
        assert!(r.errors.is_empty() && r.deferred.is_empty(), "{r:?}");
    }
}

impl Drop for Biblioteca {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copiar(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == "README.md") {
            continue;
        }
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copiar(&path, &target);
        } else {
            std::fs::copy(path, target).unwrap();
        }
    }
}

#[test]
fn busqueda_evidencia_coleccion_y_proteccion_de_la_biblioteca() {
    let lib = Biblioteca::nueva();
    let mut store = Store::open_in_memory().unwrap();
    lib.indexar(&mut store);
    let apuntes = lib.id(&store, "01 estudios/apuntes-redes.md");
    let pdf = lib.id(&store, "01 estudios/constancia-matricula.pdf");
    let anexo = lib.id(&store, "02 tramites/anexo-cita.txt");
    let paquete = lib.id(&store, "02 tramites/expediente-AURORA.txt");
    let cita = lib.id(&store, "02 tramites/cita-consulado.html");
    let pasaporte = lib.id(
        &store,
        "05 identidad ficticia/Pasaporte de Lidia Robles.txt",
    );
    let tarjeta = lib.id(&store, "04 finanzas/nota-tarjeta-prueba.txt");
    let recibo = lib.id(&store, "04 finanzas/recibo-septiembre.txt");
    let copia = lib.id(&store, "04 finanzas/recibo-septiembre-copia.txt");

    assert_eq!(
        store.sensitivity(&pasaporte).unwrap(),
        Sensitivity::Sensitive
    );
    assert_eq!(store.sensitivity(&tarjeta).unwrap(), Sensitivity::Sensitive);
    let duplicates = store
        .query(
            None,
            Some("duplicate_of"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(duplicates.len(), 1);
    assert!(duplicates[0].subject == recibo || duplicates[0].subject == copia);
    let parts = store
        .query(
            Some(&anexo),
            Some("part_of"),
            TimePoint::at(obrain_scan::now_ms()),
        )
        .unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].object.as_entity(), Some(&paquete));
    assert!(
        store
            .query(
                Some(&cita),
                Some("part_of"),
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .is_empty(),
        "compartir el tema no prueba que un documento esté contenido"
    );

    let mut brain = Brain::from_store(store);
    let found = brain.search("nodo azafrán", 10).unwrap();
    assert!(!found.semantic && found.branches == ["bm25"]);
    assert!(found.hits.iter().any(|h| h.entity == apuntes.0));
    let pdf_text = brain.preview(&pdf, None).unwrap().text.unwrap();
    assert!(pdf_text.contains("laboratorio de redes"));
    let ficha = brain.entity(&anexo).unwrap();
    assert!(
        ficha
            .observed
            .iter()
            .any(|c| c.predicate == "part_of" && c.evidence.is_some())
    );
    assert!(
        brain
            .entity(&tarjeta)
            .unwrap()
            .observed
            .iter()
            .filter_map(|c| c.evidence.as_deref())
            .all(|e| !e.contains("4111")),
        "la evidencia persistente no debe copiar el número de prueba"
    );

    let collection = EntityId(
        brain
            .create_collection("Trámite AURORA", "user@test")
            .unwrap(),
    );
    assert_eq!(
        brain
            .add_to_collection(&collection, &[anexo, pdf.clone(), paquete], "user@test")
            .unwrap(),
        3
    );
    assert_eq!(brain.collection(&collection).unwrap().rows.len(), 3);
    let filtered = brain.search_in("matrícula", 10, Some(&collection)).unwrap();
    assert!(filtered.hits.iter().any(|h| h.entity == pdf.0));
    assert!(filtered.hits.iter().all(|h| h.entity != apuntes.0));

    brain.set_pin("482916", None).unwrap();
    brain.lock();
    assert!(brain.preview(&pasaporte, None).unwrap().text.is_none());
    brain.unlock("482916").unwrap();
    assert!(brain.preview(&pasaporte, None).unwrap().text.is_some());
}

#[test]
fn actualizar_revoca_copia_y_conserva_identidad_del_contrato() {
    let lib = Biblioteca::nueva();
    let mut store = Store::open_in_memory().unwrap();
    lib.indexar(&mut store);
    let contrato = lib.id(&store, "03 contratos/Contrato alquiler ficticio.txt");
    let apuntes = lib.ruta("01 estudios/apuntes-redes.md");
    let copia = lib.ruta("04 finanzas/recibo-septiembre-copia.txt");
    let nuevo = lib.ruta("02 tramites/Contrato alquiler revisado.txt");
    std::fs::write(
        &apuntes,
        format!(
            "{}\n\nLa revisión posterior compara una topología de respaldo con el trazado inicial.",
            std::fs::read_to_string(&apuntes).unwrap()
        ),
    )
    .unwrap();
    std::fs::write(
        &copia,
        format!(
            "{}\n\nEsta copia incorpora una anotación posterior y ya no es idéntica al recibo original.",
            std::fs::read_to_string(&copia).unwrap()
        ),
    )
    .unwrap();
    std::fs::rename(
        lib.ruta("03 contratos/Contrato alquiler ficticio.txt"),
        &nuevo,
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    lib.indexar(&mut store);

    assert_eq!(
        lib.id(&store, "02 tramites/Contrato alquiler revisado.txt"),
        contrato
    );
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
    let brain = Brain::from_store(store);
    assert!(
        brain
            .search("topología de respaldo", 10)
            .unwrap()
            .hits
            .iter()
            .any(|h| h.label == "apuntes-redes.md")
    );
    assert!(brain.entity(&contrato).unwrap().label.contains("revisado"));
}
