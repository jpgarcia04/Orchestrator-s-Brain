//! Oráculo documental de la biblioteca sintética de producto.
//! Usa el pipeline real, sin modelo ni interfaz; no acredita el instalador.

use obrain_api::{Brain, Error, indexar};
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
fn recordar_documentos_con_frases_y_comprobar_su_prueba() {
    let lib = Biblioteca::nueva();
    let mut store = Store::open_in_memory().unwrap();
    lib.indexar(&mut store);
    let mut brain = Brain::from_store(store);

    let cases: [(&str, &[&str]); 5] = [
        (
            "El contrato donde decía cuándo terminaba el alquiler",
            &["Contrato alquiler ficticio.txt"],
        ),
        (
            "Los apuntes de la práctica donde hablábamos de rutas de red",
            &["apuntes-redes.md"],
        ),
        (
            "El documento del trámite que mencionaba la matrícula",
            &["constancia-matricula.pdf", "expediente-AURORA.txt"],
        ),
        (
            "El recibo correspondiente al alquiler de septiembre",
            &["recibo-septiembre.txt"],
        ),
        (
            "El archivo que tenía información sobre una cita de consulado",
            &["cita-consulado.html"],
        ),
    ];
    for (query, expected) in cases {
        let answer = brain.search(query, 10).unwrap();
        assert!(!answer.semantic && answer.branches == ["bm25"]);
        let labels: Vec<&str> = answer.hits.iter().map(|h| h.label.as_str()).collect();
        if query.contains("recibo correspondiente") {
            assert!(
                labels[..2].contains(&"recibo-septiembre.txt")
                    && labels[..2].contains(&"recibo-septiembre-copia.txt"),
                "{query}: {labels:?}"
            );
        } else {
            assert!(labels.starts_with(expected), "{query}: {labels:?}");
        }
        let proof_count = if query.contains("recibo correspondiente") {
            2
        } else {
            expected.len()
        };
        for hit in answer.hits.iter().take(proof_count) {
            assert!(!hit.snippet.is_empty(), "sin prueba: {query}");
            assert_eq!(hit.ranks.len(), 1);
            assert_eq!(hit.ranks[0].0, "bm25");
            let text = brain
                .preview(&EntityId(hit.entity.clone()), None)
                .unwrap()
                .text
                .unwrap();
            assert!(
                text.contains(&hit.snippet),
                "la prueba de {} no es texto literal del documento",
                hit.label
            );
        }
    }

    // Una consulta corta y específica conserva el comportamiento exacto.
    let exact = brain.search("nodo azafrán", 10).unwrap();
    assert_eq!(exact.hits[0].label, "apuntes-redes.md");
    assert!(exact.hits[0].snippet.contains("nodo"));
    assert!(exact.hits[0].snippet.contains("azafrán"));

    let collection = EntityId(
        brain
            .create_collection("Trámite AURORA", "user@test")
            .unwrap(),
    );
    let pdf = lib.id(brain.store(), "01 estudios/constancia-matricula.pdf");
    brain
        .add_to_collection(&collection, &[pdf.clone()], "user@test")
        .unwrap();
    let within = brain
        .search_in(
            "El documento del trámite que mencionaba la matrícula",
            10,
            Some(&collection),
        )
        .unwrap();
    assert_eq!(within.hits.len(), 1);
    assert_eq!(within.hits[0].entity, pdf.0);

    brain.set_pin("482916", None).unwrap();
    brain.lock();
    let locked = brain
        .search("El contrato donde decía cuándo terminaba el alquiler", 10)
        .unwrap();
    assert_eq!(locked.hits[0].label, "Contrato alquiler ficticio.txt");
    assert!(locked.hits[0].snippet.is_empty());
}

#[test]
fn pasajes_literales_multiples_y_navegacion_unicode() {
    let lib = Biblioteca::nueva();
    let ruta = lib.ruta("03 contratos/pasajes-unicode.txt");
    let relleno: String = (0..35)
        .map(|n| format!("Nota {n}: la biblioteca ficticia registra una observación cotidiana de vivienda sin una fecha relevante.\n\n"))
        .collect();
    std::fs::write(
        &ruta,
        format!(
            "El contrato de alquiler de Ñandú empieza en enero.\n\n{relleno}\n\nEl contrato de alquiler de Muñoz termina en otoño.\n\n{relleno}"
        ),
    )
    .unwrap();
    let mut store = Store::open_in_memory().unwrap();
    lib.indexar(&mut store);
    let id = lib.id(&store, "03 contratos/pasajes-unicode.txt");
    assert!(
        obrain_extract::index::chunks_of(store.conn(), &id)
            .unwrap()
            .len()
            > 1,
        "estado de extracción: {:?}",
        obrain_extract::index::previous(store.conn(), &id, "paragraph-pack", 1).unwrap()
    );
    let brain = Brain::from_store(store);
    let answer = brain.search("contrato alquiler", 12).unwrap();
    let hit = answer
        .hits
        .iter()
        .find(|h| h.entity == id.0)
        .unwrap_or_else(|| {
            panic!(
                "falta documento; resultados: {:?}",
                answer.hits.iter().map(|h| &h.label).collect::<Vec<_>>()
            )
        });
    assert_eq!(hit.passages.len(), 2);
    let doc = obrain_extract::index::document_text(brain.store().conn(), &id)
        .unwrap()
        .unwrap();
    for p in &hit.passages {
        assert_eq!(doc.get(p.span[0]..p.span[1]), Some(p.text.as_str()));
        let preview = brain
            .preview_at_version(
                &id,
                None,
                Some((p.span[0], p.span[1])),
                Some(&p.published_hash),
                Some(&p.text_hash),
            )
            .unwrap();
        let shown = preview.text.unwrap();
        let (start, end) = preview.highlight.unwrap();
        assert_eq!(shown.get(start..end), Some(p.text.as_str()));
    }
    assert!(
        hit.passages[0].span[1] <= hit.passages[1].span[0]
            || hit.passages[1].span[1] <= hit.passages[0].span[0]
    );
}

#[test]
fn pasaje_indexado_no_afirma_vigencia_del_original_y_respeta_pin() {
    let lib = Biblioteca::nueva();
    let mut store = Store::open_in_memory().unwrap();
    lib.indexar(&mut store);
    let id = lib.id(&store, "03 contratos/Contrato alquiler ficticio.txt");
    let brain = Brain::from_store(store);
    let answer = brain
        .search("El contrato donde decía cuándo terminaba el alquiler", 10)
        .unwrap();
    let passage = &answer
        .hits
        .iter()
        .find(|h| h.entity == id.0)
        .unwrap()
        .passages[0];
    assert!(
        passage.text.contains("31 de agosto de 2027"),
        "el pasaje no permite comprobar la fecha: {}",
        passage.text
    );
    std::fs::write(
        lib.ruta("03 contratos/Contrato alquiler ficticio.txt"),
        "Edición externa sintética todavía no indexada.",
    )
    .unwrap();
    assert!(
        brain
            .path_to_open_version(&id, Some(&passage.published_hash))
            .unwrap()
            .is_none()
    );
    let old = brain
        .preview_at_version(
            &id,
            None,
            Some((passage.span[0], passage.span[1])),
            Some(&passage.published_hash),
            Some(&passage.text_hash),
        )
        .unwrap();
    assert!(old.text.unwrap().contains(&passage.text));

    brain.store().conn().execute(
        "UPDATE content_publication SET published_hash='otra-version-sintetica' WHERE entity=?1",
        [id.as_str()],
    ).unwrap();
    assert!(matches!(
        brain.preview_at_version(
            &id,
            None,
            Some((passage.span[0], passage.span[1])),
            Some(&passage.published_hash),
            Some(&passage.text_hash)
        ),
        Err(Error::EvidenceChanged)
    ));
    brain
        .store()
        .conn()
        .execute(
            "UPDATE content_publication SET published_hash=?2 WHERE entity=?1",
            obrain_core::rusqlite::params![id.as_str(), passage.published_hash],
        )
        .unwrap();
    brain
        .store()
        .conn()
        .execute(
            "UPDATE extractions SET text_hash='otro-texto-sintetico' WHERE entity=?1",
            [id.as_str()],
        )
        .unwrap();
    assert!(matches!(
        brain.preview_at_version(
            &id,
            None,
            Some((passage.span[0], passage.span[1])),
            Some(&passage.published_hash),
            Some(&passage.text_hash)
        ),
        Err(Error::EvidenceChanged)
    ));

    brain.set_pin("482916", None).unwrap();
    brain.lock();
    let locked = brain.search("contrato alquiler", 10).unwrap();
    let locked_hit = locked.hits.iter().find(|h| h.entity == id.0).unwrap();
    assert!(locked_hit.passages.is_empty());
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
