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
