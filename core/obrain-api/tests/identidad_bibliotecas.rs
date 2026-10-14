//! Regresiones del reemplazo de una ruta mientras su antiguo archivo se mueve
//! a otra biblioteca. Sólo usa archivos sintéticos y una base en memoria.

use obrain_api::indexar;
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint, Value};
use std::path::{Path, PathBuf};

struct Fixture {
    dir: PathBuf,
    a: PathBuf,
    b: PathBuf,
    store: Store,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "obrain-identidad-{label}-{}-{}",
            std::process::id(),
            obrain_scan::now_ms()
        ));
        let a = dir.join("A");
        let b = dir.join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        Self {
            dir,
            a,
            b,
            store: Store::open_in_memory().unwrap(),
        }
    }

    fn index(&mut self, roots: &[PathBuf]) -> obrain_api::IndexReport {
        indexar::index_with(&mut self.store, roots, None, None).unwrap()
    }

    fn entity(&self, path: &Path) -> EntityId {
        self.store
            .find_subject_by_value(
                "path_observed_at",
                &path.to_string_lossy(),
                TimePoint::at(obrain_scan::now_ms()),
            )
            .unwrap()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn mover_y_reemplazar(label: &str, a_primero: bool) {
    let mut f = Fixture::new(label);
    let (origen, destino) = if a_primero {
        (f.a.clone(), f.b.clone())
    } else {
        (f.b.clone(), f.a.clone())
    };
    let anterior = origen.join("expediente.txt");
    let movido = destino.join("traslado.txt");
    std::fs::write(&anterior, "documento original con historia propia").unwrap();
    f.index(std::slice::from_ref(&origen));
    let original = f.entity(&anterior);
    let coleccion = f.store.create_entity(obrain_scan::now_ms()).unwrap();
    let relacionado = f.store.create_entity(obrain_scan::now_ms()).unwrap();
    let at = obrain_scan::now_ms();
    for (predicado, objeto) in [
        ("expires_on", Value::Text("2030-05-01".into())),
        ("sensitivity_level", Value::Text("vault".into())),
        ("member_of", Value::Entity(coleccion.clone())),
        ("attached_to", Value::Entity(relacionado.clone())),
    ] {
        f.store
            .assert(NewAssertion::new(
                original.clone(),
                predicado,
                objeto,
                Method::User,
                at,
            ))
            .unwrap();
    }
    f.store
        .set_sensitivity(&original, Sensitivity::Vault, at)
        .unwrap();
    let history_before = f.store.log_len().unwrap();

    std::fs::rename(&anterior, &movido).unwrap();
    std::fs::write(&anterior, "otro documento completamente distinto").unwrap();
    let report = f.index(&[origen.clone(), destino]);
    let replacement = f.entity(&anterior);
    assert_eq!(
        f.entity(&movido),
        original,
        "el movimiento conserva su EntityId"
    );
    assert_ne!(
        replacement, original,
        "el reemplazo necesita su propio EntityId"
    );
    assert_eq!(report.files_new, 1);
    assert_eq!(report.trashed, 0);
    assert_eq!(f.store.sensitivity(&original).unwrap(), Sensitivity::Vault);
    assert_eq!(
        f.store.sensitivity(&replacement).unwrap(),
        Sensitivity::Normal
    );
    let tp = TimePoint::at(obrain_scan::now_ms());
    for predicado in [
        "expires_on",
        "sensitivity_level",
        "member_of",
        "attached_to",
    ] {
        assert!(
            f.store.resolve(&original, predicado, tp).unwrap().is_some(),
            "historia original: {predicado}"
        );
        assert!(
            f.store
                .resolve(&replacement, predicado, tp)
                .unwrap()
                .is_none(),
            "sin herencia ajena: {predicado}"
        );
    }
    assert!(f.store.log_len().unwrap() > history_before);
    let log = f.store.log_len().unwrap();
    let again = f.index(&[origen, if a_primero { f.b.clone() } else { f.a.clone() }]);
    assert_eq!(again.files_new, 0);
    assert_eq!(f.entity(&anterior), replacement);
    assert_eq!(f.entity(&movido), original);
    assert_eq!(
        f.store.log_len().unwrap(),
        log,
        "segunda pasada idempotente"
    );
}

#[test]
fn identidad_entre_bibliotecas_a_luego_b() {
    mover_y_reemplazar("a-b", true);
}

#[test]
fn identidad_entre_bibliotecas_b_luego_a() {
    mover_y_reemplazar("b-a", false);
}

#[test]
fn biblioteca_ausente_aplaza_la_reconciliacion_sin_escrituras() {
    let mut f = Fixture::new("ausente");
    let old = f.a.join("expediente.txt");
    std::fs::write(&old, "documento anterior").unwrap();
    f.index(&[f.a.clone(), f.b.clone()]);
    let id = f.entity(&old);
    let before = f.store.log_len().unwrap();
    std::fs::rename(&f.b, f.dir.join("B-desconectada")).unwrap();
    std::fs::remove_file(&old).unwrap();
    std::fs::write(&old, "documento reemplazante").unwrap();
    let r = f.index(std::slice::from_ref(&f.a.clone()));
    assert_eq!(r.missing.len(), 1);
    assert_eq!(r.files_new, 0);
    assert_eq!(r.trashed, 0);
    assert_eq!(f.store.log_len().unwrap(), before);
    assert_eq!(f.entity(&old), id);
}

#[test]
fn raices_solapadas_ingestan_cada_observacion_una_vez() {
    let mut f = Fixture::new("solapadas");
    let nested = f.a.join("Dentro");
    std::fs::create_dir_all(&nested).unwrap();
    let path = nested.join("unico.txt");
    std::fs::write(&path, "un documento sintético").unwrap();
    let first = f.index(&[f.a.clone(), nested.clone()]);
    assert_eq!(first.files_seen, 1);
    assert_eq!(first.files_new, 1);
    let id = f.entity(&path);
    let log = f.store.log_len().unwrap();
    let second = f.index(&[nested, f.a.clone()]);
    assert_eq!(second.files_new, 0);
    assert_eq!(f.entity(&path), id);
    assert_eq!(f.store.log_len().unwrap(), log);
}

#[test]
fn identidad_aplazada_no_llega_a_busqueda_ni_a_presentacion() {
    let mut f = Fixture::new("aplazada");
    let path = f.a.join("documento.txt");
    std::fs::write(&path, "palabraexclusiva del documento").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let original = f.entity(&path);
    let tp = TimePoint::at(obrain_scan::now_ms());
    assert!(
        obrain_extract::search(&f.store, "palabraexclusiva", 10, tp)
            .unwrap()
            .iter()
            .any(|h| h.entity == original)
    );
    let (mut candidates, _) = obrain_scan::Scanner::new().scan(&f.a);
    candidates[0].identity = None;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let at = obrain_scan::now_ms();
    let mut report = obrain_scan::IngestReport::default();
    f.store
        .transaction(|s| obrain_scan::ingest(s, &candidates, at, &mut report))
        .unwrap();
    assert_eq!(report.deferred.len(), 1);
    let tp = TimePoint::at(obrain_scan::now_ms());
    assert!(
        obrain_extract::search(&f.store, "palabraexclusiva", 10, tp)
            .unwrap()
            .is_empty()
    );
    assert!(
        obrain_extract::search(&f.store, "El documento que tenía palabraexclusiva", 10, tp)
            .unwrap()
            .is_empty()
    );
    assert!(
        !obrain_link::load_files(&f.store, tp)
            .unwrap()
            .iter()
            .any(|v| v.entity == original)
    );
    assert!(
        f.store
            .resolve(&original, "identity_pending_path", tp)
            .unwrap()
            .is_some()
    );
}

#[test]
fn identidad_aplazada_sale_del_arbol_real_y_de_recientes() {
    let mut f = Fixture::new("arbol-pendiente");
    let path = f.a.join("documento.txt");
    std::fs::write(&path, "texto sintetico del arbol").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let id = f.entity(&path);
    let at = TimePoint::at(obrain_scan::now_ms());
    let folder = f
        .store
        .resolve(&id, "contained_by", at)
        .unwrap()
        .unwrap()
        .object
        .as_entity()
        .unwrap()
        .clone();
    let (mut candidates, _) = obrain_scan::Scanner::new().scan(&f.a);
    candidates[0].identity = None;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut report = obrain_scan::IngestReport::default();
    f.store
        .transaction(|s| obrain_scan::ingest(s, &candidates, obrain_scan::now_ms(), &mut report))
        .unwrap();
    assert_eq!(report.deferred.len(), 1);
    let at = TimePoint::at(obrain_scan::now_ms());
    assert!(f.store.resolve(&id, "contained_by", at).unwrap().is_some());
    let listing = obrain_api::sky::children(&f.store, Some(&folder), at).unwrap();
    assert!(
        !listing.rows.iter().any(|r| r.id == id.0),
        "el árbol todavía presenta la identidad pendiente"
    );
    assert!(
        !obrain_api::sky::recent(&f.store, 10, at)
            .unwrap()
            .iter()
            .any(|r| r.id == id.0)
    );
}

#[test]
fn identidad_aplazada_no_revela_chunks_por_api_de_previsualizacion() {
    let mut f = Fixture::new("preview-pendiente");
    let path = f.a.join("documento.txt");
    std::fs::write(&path, "contenido reservado y sintetico").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let id = f.entity(&path);
    let (mut candidates, _) = obrain_scan::Scanner::new().scan(&f.a);
    candidates[0].identity = None;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut report = obrain_scan::IngestReport::default();
    f.store
        .transaction(|s| obrain_scan::ingest(s, &candidates, obrain_scan::now_ms(), &mut report))
        .unwrap();
    assert_eq!(report.deferred.len(), 1);
    let brain = obrain_api::Brain::from_store(std::mem::replace(
        &mut f.store,
        Store::open_in_memory().unwrap(),
    ));
    assert!(
        brain.preview(&id, None).is_err(),
        "la API devolvió los chunks conservados"
    );
    assert!(
        brain.entity(&id).is_err(),
        "el acceso directo mostró su ficha"
    );
    assert!(!brain.may_open(&id).unwrap());
    assert!(brain.path_to_open(&id).unwrap().is_none());
}

#[test]
fn retirada_y_recuperacion_alcanzan_coleccion_mapa_y_previsualizacion() {
    let mut f = Fixture::new("superficies-recuperacion");
    let path = f.a.join("documento.txt");
    std::fs::write(&path, "contenido historico y sintetico").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let id = f.entity(&path);
    let now = obrain_scan::now_ms();
    let collection = obrain_api::colecciones::crear(&f.store, "Prueba", now, "fixture").unwrap();
    obrain_api::colecciones::meter(
        &f.store,
        &collection,
        std::slice::from_ref(&id),
        now,
        "fixture",
    )
    .unwrap();
    f.store
        .assert(
            NewAssertion::new(id.clone(), "expires_on", "2031-05-01", Method::User, now)
                .source("fixture:decision")
                .evidence("fecha elegida en fixture"),
        )
        .unwrap();
    f.store
        .set_sensitivity(&id, Sensitivity::Vault, now)
        .unwrap();
    let folder = f
        .store
        .resolve(&id, "contained_by", TimePoint::at(now))
        .unwrap()
        .unwrap()
        .object
        .as_entity()
        .unwrap()
        .clone();
    let (mut candidates, _) = obrain_scan::Scanner::new().scan(&f.a);
    candidates[0].identity = None;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut report = obrain_scan::IngestReport::default();
    f.store
        .transaction(|s| obrain_scan::ingest(s, &candidates, obrain_scan::now_ms(), &mut report))
        .unwrap();
    assert_eq!(report.deferred.len(), 1);
    let at = TimePoint::at(obrain_scan::now_ms());
    assert!(f.store.resolve(&id, "contained_by", at).unwrap().is_some());
    assert!(f.store.resolve(&id, "member_of", at).unwrap().is_some());
    assert!(f.store.resolve(&id, "expires_on", at).unwrap().is_some());
    assert_eq!(f.store.sensitivity(&id).unwrap(), Sensitivity::Vault);
    assert_eq!(obrain_api::sky::regions(&f.store, 30, at).unwrap().files, 0);
    assert!(obrain_api::sky::sensitive(&f.store, at).unwrap().is_empty());
    assert_eq!(
        obrain_api::colecciones::todas(&f.store, at).unwrap()[0].documents,
        0
    );
    let opened = obrain_api::colecciones::abrir(&f.store, &collection, at).unwrap();
    assert!(opened.rows.is_empty() && opened.in_trash.is_empty());
    let lens = obrain_api::lens::open(&f.store, std::slice::from_ref(&folder), 1, 50, at).unwrap();
    assert!(!lens.nodes.iter().any(|n| n.id == id.0));
    let direct_lens =
        obrain_api::lens::open(&f.store, std::slice::from_ref(&id), 1, 50, at).unwrap();
    assert!(direct_lens.nodes.is_empty());

    let recovered = f.index(std::slice::from_ref(&f.a.clone()));
    assert_eq!(recovered.files_new, 0);
    assert_eq!(f.entity(&path), id);
    let at = TimePoint::at(obrain_scan::now_ms());
    assert!(
        f.store
            .resolve(&id, "identity_pending_path", at)
            .unwrap()
            .is_none()
    );
    let decision = f.store.resolve(&id, "expires_on", at).unwrap().unwrap();
    assert_eq!(decision.source, "fixture:decision");
    assert_eq!(
        decision.evidence.as_deref(),
        Some("fecha elegida en fixture")
    );
    assert!(
        obrain_api::sky::children(&f.store, Some(&folder), at)
            .unwrap()
            .rows
            .iter()
            .any(|r| r.id == id.0)
    );
    assert_eq!(
        obrain_api::colecciones::todas(&f.store, at).unwrap()[0].documents,
        1
    );
    assert_eq!(
        obrain_api::colecciones::abrir(&f.store, &collection, at)
            .unwrap()
            .rows
            .len(),
        1
    );
    let brain = obrain_api::Brain::from_store(std::mem::replace(
        &mut f.store,
        Store::open_in_memory().unwrap(),
    ));
    let preview = brain.preview(&id, None).unwrap();
    assert_eq!(
        preview.text.as_deref(),
        Some("contenido historico y sintetico")
    );
    assert!(
        brain
            .entity(&id)
            .unwrap()
            .collections
            .iter()
            .any(|c| c.id == collection.0)
    );
    assert!(brain.path_to_open(&id).unwrap().is_some());
}

#[test]
fn papelera_sin_identidad_pendiente_conserva_su_vista_historica() {
    let mut f = Fixture::new("papelera-distinta");
    let path = f.a.join("documento.txt");
    std::fs::write(&path, "texto historico de papelera").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let id = f.entity(&path);
    std::fs::remove_file(&path).unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let at = TimePoint::at(obrain_scan::now_ms());
    assert!(
        f.store
            .resolve(&id, "path_observed_at", at)
            .unwrap()
            .is_none()
    );
    assert!(
        f.store
            .resolve(&id, "identity_pending_path", at)
            .unwrap()
            .is_none()
    );
    assert!(
        obrain_api::papelera::of(&f.store, &id, at)
            .unwrap()
            .is_some()
    );
    let brain = obrain_api::Brain::from_store(std::mem::replace(
        &mut f.store,
        Store::open_in_memory().unwrap(),
    ));
    assert!(brain.entity(&id).is_ok());
    assert!(brain.preview(&id, None).is_ok());
}

#[test]
fn relaciones_directas_no_presentan_un_documento_pendiente() {
    let mut f = Fixture::new("relaciones-pendientes");
    let package_path = f.a.join("paquete.txt");
    let part_path = f.a.join("parte.txt");
    std::fs::write(&package_path, "paquete sintetico").unwrap();
    std::fs::write(&part_path, "parte sintetica").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let package = f.entity(&package_path);
    let part = f.entity(&part_path);
    let now = obrain_scan::now_ms();
    f.store
        .assert(NewAssertion::new(
            part.clone(),
            obrain_core::PART_OF,
            Value::Entity(package.clone()),
            Method::Inferred,
            now,
        ))
        .unwrap();
    f.store
        .assert(NewAssertion::new(
            package.clone(),
            "attached_to",
            Value::Entity(part.clone()),
            Method::User,
            now,
        ))
        .unwrap();
    assert_eq!(
        obrain_api::dentro::parts(&f.store, &package, TimePoint::at(now))
            .unwrap()
            .len(),
        1
    );
    let (mut candidates, _) = obrain_scan::Scanner::new().scan(&f.a);
    candidates
        .iter_mut()
        .find(|c| c.path == part_path)
        .unwrap()
        .identity = None;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut report = obrain_scan::IngestReport::default();
    f.store
        .transaction(|s| obrain_scan::ingest(s, &candidates, obrain_scan::now_ms(), &mut report))
        .unwrap();
    assert_eq!(report.deferred.len(), 1);
    let at = TimePoint::at(obrain_scan::now_ms());
    assert!(
        f.store
            .resolve(&part, obrain_core::PART_OF, at)
            .unwrap()
            .is_some()
    );
    assert!(
        f.store
            .resolve(&package, "attached_to", at)
            .unwrap()
            .is_some()
    );
    assert!(
        obrain_api::dentro::parts(&f.store, &package, at)
            .unwrap()
            .is_empty()
    );
    assert!(
        obrain_api::dentro::packages(&f.store, at)
            .unwrap()
            .iter()
            .all(|p| p.parts.is_empty())
    );
    let view = obrain_api::entity::view(&f.store, &package, at).unwrap();
    assert!(
        view.observed
            .iter()
            .chain(&view.inferred)
            .all(|c| c.value_entity.as_deref() != Some(part.0.as_str()))
    );
    let lens = obrain_api::lens::open(&f.store, std::slice::from_ref(&package), 1, 50, at).unwrap();
    assert!(!lens.nodes.iter().any(|n| n.id == part.0));
}

#[test]
fn movimiento_aplazado_conserva_historia_y_luego_se_recupera() {
    let mut f = Fixture::new("recupera");
    let old = f.a.join("expediente.txt");
    let moved = f.b.join("expediente.txt");
    std::fs::write(&old, "contenido que se conserva").unwrap();
    f.index(std::slice::from_ref(&f.a.clone()));
    let original = f.entity(&old);
    let at = obrain_scan::now_ms();
    f.store
        .assert(NewAssertion::new(
            original.clone(),
            "expires_on",
            "2031-05-01",
            Method::User,
            at,
        ))
        .unwrap();
    std::fs::rename(&old, &moved).unwrap();
    let (mut candidates, _) = obrain_scan::Scanner::new().scan(&f.b);
    candidates[0].identity = None;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let at = obrain_scan::now_ms();
    let absent = obrain_scan::ausentes::observe_missing(&f.store, &f.a, at).unwrap();
    let mut report = obrain_scan::IngestReport::default();
    f.store
        .transaction(|s| {
            obrain_scan::ingest(s, &candidates, at, &mut report)?;
            obrain_scan::ausentes::hold_missing(s, &absent, &report.seen, at)
        })
        .unwrap();
    assert_eq!(report.deferred.len(), 1);
    let tp = TimePoint::at(obrain_scan::now_ms());
    assert!(
        obrain_link::load_files(&f.store, tp)
            .unwrap()
            .iter()
            .all(|v| v.entity != original)
    );
    assert!(
        f.store
            .resolve(&original, "missing_since", tp)
            .unwrap()
            .is_none()
    );
    let r = f.index(&[f.a.clone(), f.b.clone()]);
    assert_eq!(r.trashed, 0);
    assert_eq!(r.files_new, 0);
    assert_eq!(f.entity(&moved), original);
    assert!(
        f.store
            .resolve(
                &original,
                "expires_on",
                TimePoint::at(obrain_scan::now_ms())
            )
            .unwrap()
            .is_some()
    );
}
