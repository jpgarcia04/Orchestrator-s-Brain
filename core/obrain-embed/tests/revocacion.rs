//! Revocación en el cambio de sensibilidad, sin ejecutar `embed_all`.
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint, Value};
use obrain_embed::{embedder::ModelId, index};
use obrain_guard::{GuardReport, classify_all};

struct Modelo;
impl obrain_embed::embedder::Embedder for Modelo {
    fn model(&self) -> ModelId {
        ModelId {
            id: "fixture",
            version: 1,
            dim: 2,
        }
    }
    fn embed(
        &self,
        texts: &[&str],
        _: obrain_embed::embedder::Role,
    ) -> obrain_embed::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0, 0.0]).collect())
    }
}

const T: i64 = 1_000;
fn base() -> Store {
    let s = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(s.conn()).unwrap();
    index::migrate(s.conn()).unwrap();
    s
}
fn archivo(s: &Store, path: &str) -> EntityId {
    let e = s.create_entity(T).unwrap();
    s.assert(NewAssertion::new(
        e.clone(),
        "path_observed_at",
        path,
        Method::Deterministic,
        T,
    ))
    .unwrap();
    s.assert(NewAssertion::new(
        e.clone(),
        "availability",
        "local",
        Method::Deterministic,
        T,
    ))
    .unwrap();
    s.conn().execute("INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text) VALUES (?1, 'fixture', 1, 0, 0, 9, 'SINTETICO')", [e.as_str()]).unwrap();
    index::insert(
        s.conn(),
        &index::Pending {
            chunk: s.conn().last_insert_rowid(),
            entity: e.clone(),
            chunker: "fixture".into(),
            chunker_ver: 1,
            text: "SINTETICO".into(),
        },
        ModelId {
            id: "fixture",
            version: 1,
            dim: 2,
        },
        &[1.0, 0.0],
    )
    .unwrap();
    for (p, v) in [
        ("embedded_with", Value::Text("fixture@1".into())),
        ("vector_count", Value::Number(1.0)),
    ] {
        s.assert(NewAssertion::new(e.clone(), p, v, Method::Deterministic, T))
            .unwrap();
    }
    e
}
fn revocado(s: &Store, e: &EntityId, at: i64) {
    let n: i64 = s
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM vectors WHERE entity = ?1",
            [e.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0, "vault conserva vectores sin modelo");
    for p in ["embedded_with", "vector_count"] {
        assert!(
            s.resolve(e, p, TimePoint::at(at)).unwrap().is_none(),
            "procedencia vigente sin vectores: {p}"
        );
        assert!(
            s.resolve(e, p, TimePoint::at(T)).unwrap().is_some(),
            "se perdió el historial: {p}"
        );
    }
}
#[test]
fn clasificacion_automatica_sin_modelo_revoca_vectores_y_procedencia() {
    let mut s = base();
    let e = archivo(&s, "fixtures/credentials.txt");
    let files = obrain_extract::load_files(&s, TimePoint::at(T + 1)).unwrap();
    s.transaction(|s| classify_all(s, &files, T + 1, false, &mut GuardReport::default()))
        .unwrap();
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Vault);
    revocado(&s, &e, T + 1);
}
#[test]
fn marca_manual_revoca_tambien_la_procedencia() {
    let s = base();
    let e = archivo(&s, "fixtures/documento.txt");
    obrain_guard::mark_by_user(&s, &e, Sensitivity::Vault, T + 1).unwrap();
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Vault);
    revocado(&s, &e, T + 1);
}

fn carpeta(s: &Store) -> EntityId {
    let e = s.create_entity(T).unwrap();
    s.assert(NewAssertion::new(
        e.clone(),
        "folder_path",
        "fixtures/restringida",
        Method::Deterministic,
        T,
    ))
    .unwrap();
    e
}
#[test]
fn herencia_de_carpeta_y_excepcion_posterior_del_usuario() {
    let s = base();
    let folder = carpeta(&s);
    let e = archivo(&s, "fixtures/restringida/documento.txt");
    let other = archivo(&s, "fixtures/otro.txt");
    obrain_guard::mark_folder(&s, &folder, Some(Sensitivity::Vault), T + 1).unwrap();
    revocado(&s, &e, T + 1);
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Vault);
    assert!(
        s.resolve(&other, "embedded_with", TimePoint::at(T + 1))
            .unwrap()
            .is_some()
    );
    for level in [Sensitivity::Normal, Sensitivity::Sensitive] {
        obrain_guard::mark_by_user(&s, &e, level, T + 2).unwrap();
        assert_eq!(s.sensitivity(&e).unwrap(), level);
        revocado(&s, &e, T + 2);
        let allowed = obrain_guard::gate::embeddable(s.conn()).unwrap();
        assert!(allowed.contains(&e));
        assert_eq!(
            index::pending(
                s.conn(),
                &allowed,
                ModelId {
                    id: "fixture",
                    version: 1,
                    dim: 2
                }
            )
            .unwrap()
            .iter()
            .filter(|p| p.entity == e)
            .count(),
            1
        );
    }
}
#[test]
fn clasificacion_actualiza_paquete_aunque_se_procese_antes_de_la_parte() {
    let s = base();
    let part = archivo(&s, "fixtures/credentials.txt");
    let package = archivo(&s, "fixtures/paquete.txt");
    s.assert(NewAssertion::new(
        part.clone(),
        "part_of",
        Value::Entity(package.clone()),
        Method::Deterministic,
        T,
    ))
    .unwrap();
    let mut files = obrain_extract::load_files(&s, TimePoint::at(T + 1)).unwrap();
    files.sort_by_key(|f| f.entity != package);
    let mut report = GuardReport::default();
    classify_all(&s, &files, T + 1, false, &mut report).unwrap();
    for e in [&part, &package] {
        assert_eq!(s.sensitivity(e).unwrap(), Sensitivity::Vault);
        revocado(&s, e, T + 1);
    }
    assert_eq!(report.effective, [0, 0, 2]);
    assert_eq!(report.from_parts, 1);
}
#[test]
fn clasificar_sin_modelo_hereda_una_marca_ya_existente() {
    let s = base();
    let folder = carpeta(&s);
    obrain_guard::mark_folder(&s, &folder, Some(Sensitivity::Vault), T).unwrap();
    // Simula un documento previamente vectorizado que ahora aparece bajo la marca.
    let e = archivo(&s, "fixtures/restringida/documento.txt");
    let files = obrain_extract::load_files(&s, TimePoint::at(T + 1)).unwrap();
    classify_all(&s, &files, T + 1, false, &mut GuardReport::default()).unwrap();
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Vault);
    revocado(&s, &e, T + 1);
}
#[test]
fn residuos_historicos_sin_vectores_se_retractan_una_sola_vez() {
    let s = base();
    let e = archivo(&s, "fixtures/documento.txt");
    s.conn()
        .execute(
            "UPDATE entities SET sensitivity = 'vault' WHERE id = ?1",
            [e.as_str()],
        )
        .unwrap();
    s.conn()
        .execute("DELETE FROM vectors WHERE entity = ?1", [e.as_str()])
        .unwrap();
    assert_eq!(s.purge_vault_embeddings(T + 1).unwrap(), vec![e.clone()]);
    revocado(&s, &e, T + 1);
    assert!(s.purge_vault_embeddings(T + 2).unwrap().is_empty());
    // La consulta histórica no cambia después de una segunda purga.
    revocado(&s, &e, T + 2);
}
#[test]
fn fallo_de_retractacion_revierte_clasificacion_y_vectores() {
    let s = base();
    let e = archivo(&s, "fixtures/credentials.txt");
    s.conn().execute_batch("CREATE TRIGGER fallo_fixture BEFORE UPDATE OF retracted_at ON assertions WHEN OLD.predicate = 'embedded_with' BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
    let n = s.log_len().unwrap();
    let files = obrain_extract::load_files(&s, TimePoint::at(T + 1)).unwrap();
    assert!(classify_all(&s, &files, T + 1, false, &mut GuardReport::default()).is_err());
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Normal);
    assert_eq!(s.log_len().unwrap(), n);
    assert_eq!(
        s.conn()
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        s.resolve(&e, "embedded_with", TimePoint::at(T + 1))
            .unwrap()
            .is_some()
    );
}
#[test]
fn fallo_en_paquete_revierte_la_marca_y_la_revocacion_de_la_parte() {
    let s = base();
    let e = archivo(&s, "fixtures/documento.txt");
    let package = archivo(&s, "fixtures/paquete.txt");
    s.assert(NewAssertion::new(
        e.clone(),
        "part_of",
        Value::Entity(package.clone()),
        Method::Deterministic,
        T,
    ))
    .unwrap();
    s.conn().execute_batch(&format!("CREATE TRIGGER fallo_fixture BEFORE UPDATE OF retracted_at ON assertions WHEN OLD.subject = '{}' AND OLD.predicate = 'vector_count' BEGIN SELECT RAISE(ABORT, 'fixture'); END;", package.as_str())).unwrap();
    let n = s.log_len().unwrap();
    assert!(obrain_guard::mark_by_user(&s, &e, Sensitivity::Vault, T + 1).is_err());
    assert_eq!(s.log_len().unwrap(), n);
    for entity in [&e, &package] {
        assert_eq!(s.sensitivity(entity).unwrap(), Sensitivity::Normal);
        assert!(
            s.resolve(entity, "embedded_with", TimePoint::at(T + 1))
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(
        s.conn()
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn bajar_de_sensitive_a_normal_conserva_vectores_y_procedencia() {
    let s = base();
    let e = archivo(&s, "fixtures/documento.txt");
    for (level, at) in [
        (Sensitivity::Sensitive, T + 1),
        (Sensitivity::Normal, T + 2),
    ] {
        obrain_guard::mark_by_user(&s, &e, level, at).unwrap();
        assert_eq!(s.sensitivity(&e).unwrap(), level);
        assert!(
            s.resolve(&e, "embedded_with", TimePoint::at(at))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            s.conn()
                .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
#[test]
fn dry_run_no_revoca_ni_cambia_el_log() {
    let s = base();
    let e = archivo(&s, "fixtures/credentials.txt");
    let n = s.log_len().unwrap();
    let files = obrain_extract::load_files(&s, TimePoint::at(T + 1)).unwrap();
    classify_all(&s, &files, T + 1, true, &mut GuardReport::default()).unwrap();
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Normal);
    assert_eq!(s.log_len().unwrap(), n);
    assert_eq!(
        s.conn()
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn sin_tabla_opcional_se_retracta_la_procedencia_y_no_se_crea_el_indice() {
    let s = Store::open_in_memory().unwrap();
    let e = s.create_entity(T).unwrap();
    s.assert(NewAssertion::new(
        e.clone(),
        "embedded_with",
        "fixture@1",
        Method::Deterministic,
        T,
    ))
    .unwrap();
    s.set_sensitivity(&e, Sensitivity::Vault, T + 1).unwrap();
    assert!(
        s.resolve(&e, "embedded_with", TimePoint::at(T + 1))
            .unwrap()
            .is_none()
    );
    assert!(
        !s.conn()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'vectors')",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap()
    );
}

#[test]
fn retorno_legitimo_recalcula_y_escribe_procedencia_nueva() {
    for level in [Sensitivity::Normal, Sensitivity::Sensitive] {
        let s = base();
        let e = archivo(&s, "fixtures/documento.txt");
        let old = s
            .resolve(&e, "embedded_with", TimePoint::at(T))
            .unwrap()
            .unwrap();
        obrain_guard::mark_by_user(&s, &e, Sensitivity::Vault, T + 1).unwrap();
        obrain_guard::mark_by_user(&s, &e, level, T + 2).unwrap();
        revocado(&s, &e, T + 2);
        let mut report = obrain_embed::EmbedReport::default();
        obrain_embed::embed_all(&s, &Modelo, T + 3, &mut report).unwrap();
        assert_eq!(report.vectors_written, 1);
        let new = s
            .resolve(&e, "embedded_with", TimePoint::at(T + 3))
            .unwrap()
            .unwrap();
        assert_ne!(new.id, old.id);
        assert_eq!(new.asserted_at, T + 3);
        assert_eq!(
            s.resolve(&e, "vector_count", TimePoint::at(T + 3))
                .unwrap()
                .unwrap()
                .object,
            Value::Number(1.0)
        );
        assert_eq!(
            s.resolve(&e, "embedded_with", TimePoint::at(T))
                .unwrap()
                .unwrap()
                .id,
            old.id
        );
    }
}

#[test]
fn quitar_marca_no_restaura_vectores_borrados() {
    let s = base();
    let folder = carpeta(&s);
    let e = archivo(&s, "fixtures/restringida/documento.txt");
    obrain_guard::mark_folder(&s, &folder, Some(Sensitivity::Vault), T + 1).unwrap();
    obrain_guard::mark_folder(&s, &folder, None, T + 2).unwrap();
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Normal);
    revocado(&s, &e, T + 2);
    let mut report = obrain_embed::EmbedReport::default();
    obrain_embed::embed_all(&s, &Modelo, T + 3, &mut report).unwrap();
    assert_eq!(report.vectors_written, 1);
}

#[test]
fn salir_de_vault_no_reutiliza_residuos_historicos() {
    let s = base();
    let e = archivo(&s, "fixtures/documento.txt");
    s.conn()
        .execute(
            "UPDATE entities SET sensitivity = 'vault' WHERE id = ?1",
            [e.as_str()],
        )
        .unwrap();
    obrain_guard::mark_by_user(&s, &e, Sensitivity::Normal, T + 1).unwrap();
    assert_eq!(s.sensitivity(&e).unwrap(), Sensitivity::Normal);
    revocado(&s, &e, T + 1);
}

#[test]
fn fallo_de_purga_revierte_la_marca_de_carpeta_completa() {
    let s = base();
    let folder = carpeta(&s);
    let e = archivo(&s, "fixtures/restringida/documento.txt");
    s.conn().execute_batch("CREATE TRIGGER fallo_fixture BEFORE DELETE ON vectors BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
    let n = s.log_len().unwrap();
    assert!(obrain_guard::mark_folder(&s, &folder, Some(Sensitivity::Vault), T + 1).is_err());
    assert_eq!(s.log_len().unwrap(), n);
    assert!(s.folder_marks(TimePoint::at(T + 1)).unwrap().is_empty());
    for entity in [&folder, &e] {
        assert_eq!(s.sensitivity(entity).unwrap(), Sensitivity::Normal);
    }
    assert!(
        s.resolve(&e, "embedded_with", TimePoint::at(T + 1))
            .unwrap()
            .is_some()
    );
    assert_eq!(
        s.conn()
            .query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
