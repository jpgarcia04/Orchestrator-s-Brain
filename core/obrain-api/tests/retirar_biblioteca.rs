//! Pruebas de retiro de bibliotecas registradas ([ADR-0018]).
//!
//! Verifica que al retirar una biblioteca:
//! - Se retracta la faceta `has_facet = "library"`.
//! - Los documentos y proyecciones exclusivos se purgan sin tocar el disco.
//! - Los documentos pertenecientes a otra biblioteca activa se conservan intactos.
//! - El explorador y las raíces reflejan el nuevo estado de la bóveda.
//! - Los fallos de limpieza provocan rollback completo de la transacción.
//! - Las identidades y duplicados en otras bibliotecas se preservan.

use obrain_api::indexar::{libraries, unregister_library};
use obrain_api::sky;
use obrain_core::{EntityId, Method, NewAssertion, Store, TimePoint};
use obrain_link::{FileView, LinkReport};
use std::path::Path;

const T: i64 = 1_700_000_000_000;

fn at() -> TimePoint {
    TimePoint::at(T + 10_000)
}

fn crear_base() -> Store {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    obrain_embed::index::migrate(store.conn()).unwrap();
    store
}

fn registrar_biblioteca(store: &Store, ruta: &str) -> EntityId {
    obrain_scan::register_library(store, Path::new(ruta), T).unwrap()
}

fn indexar_archivos(store: &Store, paths: &[&str]) -> Vec<EntityId> {
    let files: Vec<FileView> = paths
        .iter()
        .map(|p| {
            let e = store.create_entity(T).unwrap();
            store
                .assert(NewAssertion::new(
                    e.clone(),
                    "path_observed_at",
                    *p,
                    Method::Deterministic,
                    T,
                ))
                .unwrap();
            FileView {
                entity: e,
                path: (*p).to_string(),
                hash: None,
                modified_ms: T,
                size: 1,
                availability: "local".into(),
            }
        })
        .collect();
    let mut r = LinkReport::default();
    obrain_link::structure::link(store, &files, T, &mut r).unwrap();
    files.into_iter().map(|f| f.entity).collect()
}

#[test]
fn retirar_biblioteca_limpia_faceta_y_documentos_exclusivos() {
    let store = crear_base();

    registrar_biblioteca(&store, r"C:\Vault\Personal");
    registrar_biblioteca(&store, r"C:\Vault\Capturas");

    let docs_a = indexar_archivos(
        &store,
        &[r"C:\Vault\Personal\doc1.txt", r"C:\Vault\Personal\doc2.txt"],
    );
    let docs_b = indexar_archivos(
        &store,
        &[
            r"C:\Vault\Capturas\rec1.mp4",
            r"C:\Vault\Capturas\sub\rec2.mp4",
        ],
    );

    // Insertar fragmentos y vectores de prueba para verificar su purga
    for e in docs_a.iter().chain(docs_b.iter()) {
        store
            .conn()
            .execute(
                "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text) VALUES (?1, 'test', 1, 0, 0, 10, 'contenido de prueba')",
                [e.as_str()],
            )
            .unwrap();
        let chunk_id = store.conn().last_insert_rowid();
        store
            .conn()
            .execute(
                "INSERT INTO vectors (chunk, entity, model_id, model_ver, dim, chunker, chunker_ver, vec) VALUES (?1, ?2, 'test-model', 1, 1, 'test', 1, X'00010203')",
                obrain_core::rusqlite::params![chunk_id, e.as_str()],
            )
            .unwrap();
    }

    let libs_antes = libraries(&store).unwrap();
    assert_eq!(libs_antes.len(), 2);

    let informe = unregister_library(&store, r"C:\Vault\Capturas", T + 1000).unwrap();
    assert_eq!(informe.documentos_retirados, 2);
    assert_eq!(informe.documentos_conservados, 0);
    assert!(informe.vigilancia_desactivada);
    assert!(informe.aviso_vigilancia.is_none());

    // 1. Las bibliotecas registradas ahora son solo Personal
    let libs_despues = libraries(&store).unwrap();
    assert_eq!(libs_despues, vec![r"C:\Vault\Personal".to_string()]);

    // 2. Documentos de Capturas ya no tienen ruta vigente ni fragmentos/vectores
    for e in &docs_b {
        assert!(
            store
                .resolve(e, "path_observed_at", at())
                .unwrap()
                .is_none(),
            "el documento de la biblioteca retirada no debe tener ruta vigente"
        );
        let n_chunks: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE entity = ?1",
                [e.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_chunks, 0, "los fragmentos de búsqueda deben ser purgados");

        let n_vecs: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM vectors WHERE entity = ?1",
                [e.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_vecs, 0, "los vectores deben ser purgados");
    }

    // 3. Documentos de Personal permanecen 100% intactos
    for e in &docs_a {
        assert!(
            store
                .resolve(e, "path_observed_at", at())
                .unwrap()
                .is_some(),
            "el documento de la otra biblioteca debe conservar su ruta"
        );
        let n_chunks: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE entity = ?1",
                [e.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_chunks, 1);
    }
}

#[test]
fn retirar_biblioteca_conserva_documentos_si_pertenecen_a_otra_biblioteca() {
    let store = crear_base();

    // Bibliotecas superpuestas / anidadas
    registrar_biblioteca(&store, r"C:\Vault");
    registrar_biblioteca(&store, r"C:\Vault\Capturas");

    let docs = indexar_archivos(&store, &[r"C:\Vault\Capturas\rec.mp4"]);
    let informe = unregister_library(&store, r"C:\Vault\Capturas", T + 1000).unwrap();

    // El documento pertenece también a C:\Vault, por lo que se conserva
    assert_eq!(informe.documentos_retirados, 0);
    assert_eq!(informe.documentos_conservados, 1);

    let libs = libraries(&store).unwrap();
    assert_eq!(libs, vec![r"C:\Vault".to_string()]);

    let e = &docs[0];
    assert!(
        store
            .resolve(e, "path_observed_at", at())
            .unwrap()
            .is_some(),
        "el documento compartido con otra biblioteca activa debe seguir vivo"
    );
}

#[test]
fn retirar_biblioteca_inexistente_devuelve_error() {
    let store = crear_base();
    registrar_biblioteca(&store, r"C:\Vault\Personal");

    let err = unregister_library(&store, r"C:\Vault\NoExiste", T + 1000).unwrap_err();
    assert!(matches!(err, obrain_api::Error::BibliotecaNoEncontrada(_)));
}

#[test]
fn retirar_ultima_biblioteca_deja_boveda_vacia() {
    let store = crear_base();
    registrar_biblioteca(&store, r"C:\Vault\Unica");
    indexar_archivos(&store, &[r"C:\Vault\Unica\nota.txt"]);

    let informe = unregister_library(&store, r"C:\Vault\Unica", T + 1000).unwrap();
    assert_eq!(informe.documentos_retirados, 1);

    assert!(libraries(&store).unwrap().is_empty());

    let exploracion = sky::children(&store, None, at()).unwrap();
    assert!(
        exploracion.rows.is_empty(),
        "al retirar la única biblioteca, el explorador debe quedar completamente vacío"
    );
}

#[test]
fn fallo_de_limpieza_revierte_transaccion_completamente() {
    let store = crear_base();
    registrar_biblioteca(&store, r"C:\Vault\Critica");
    let docs = indexar_archivos(&store, &[r"C:\Vault\Critica\doc.txt"]);
    let doc_entity = docs[0].clone();

    // Insertar fragmentos y vectores
    store
        .conn()
        .execute(
            "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text) VALUES (?1, 'test', 1, 0, 0, 10, 'contenido crítico')",
            [doc_entity.as_str()],
        )
        .unwrap();
    let chunk_id = store.conn().last_insert_rowid();
    store
        .conn()
        .execute(
            "INSERT INTO vectors (chunk, entity, model_id, model_ver, dim, chunker, chunker_ver, vec) VALUES (?1, ?2, 'test-model', 1, 1, 'test', 1, X'00010203')",
            obrain_core::rusqlite::params![chunk_id, doc_entity.as_str()],
        )
        .unwrap();

    // Crear un trigger que fuerza un fallo de base de datos durante la purga en chunks
    store
        .conn()
        .execute(
            "CREATE TRIGGER fallo_forzado_purge BEFORE DELETE ON chunks
             BEGIN
                 SELECT RAISE(ABORT, 'error simulado de base de datos durante purga');
             END;",
            [],
        )
        .unwrap();

    let mut brain = obrain_api::Brain::from_store(store);

    // Intentar retirar la biblioteca: debe fallar por el error en purge y hacer rollback
    let res = brain.unregister_library(r"C:\Vault\Critica");
    assert!(res.is_err(), "debe fallar y no comunicarse como exitoso");

    // Verificar que la transacción fue revertida en su totalidad
    let s = brain.store();
    let libs = libraries(s).unwrap();
    assert_eq!(
        libs,
        vec![r"C:\Vault\Critica".to_string()],
        "la biblioteca debe seguir registrada tras el rollback"
    );

    assert!(
        s.resolve(&doc_entity, "path_observed_at", at())
            .unwrap()
            .is_some(),
        "la ubicación del documento debe conservarse tras el rollback"
    );

    let n_chunks: i64 = s
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE entity = ?1",
            [doc_entity.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        n_chunks, 1,
        "los fragmentos no deben borrarse parcialmente si la transacción falla"
    );

    let n_vecs: i64 = s
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM vectors WHERE entity = ?1",
            [doc_entity.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        n_vecs, 1,
        "los vectores no deben borrarse parcialmente si la transacción falla"
    );
}

#[test]
fn retirar_biblioteca_con_duplicados_preserva_identidades_en_otras_bibliotecas() {
    let store = crear_base();
    registrar_biblioteca(&store, r"C:\Vault\A");
    registrar_biblioteca(&store, r"C:\Vault\B");
    registrar_biblioteca(&store, r"C:\Vault\C");

    let doc_a = indexar_archivos(&store, &[r"C:\Vault\A\contrato.pdf"])[0].clone();
    let doc_b = indexar_archivos(&store, &[r"C:\Vault\B\contrato_copia.pdf"])[0].clone();
    let doc_c = indexar_archivos(&store, &[r"C:\Vault\C\nota.txt"])[0].clone();

    // Relacionar doc_b como duplicado de doc_a (ADR-0026)
    store
        .assert(NewAssertion::new(
            doc_b.clone(),
            "duplicate_of",
            doc_a.as_str(),
            Method::Deterministic,
            T,
        ))
        .unwrap();

    let informe = unregister_library(&store, r"C:\Vault\A", T + 1000).unwrap();
    assert_eq!(informe.documentos_retirados, 1);
    assert_eq!(informe.documentos_conservados, 0);

    // Bibliotecas B y C siguen intactas
    let libs = libraries(&store).unwrap();
    assert_eq!(libs.len(), 2);
    assert!(libs.contains(&r"C:\Vault\B".to_string()));
    assert!(libs.contains(&r"C:\Vault\C".to_string()));

    // doc_a fue retirado
    assert!(
        store
            .resolve(&doc_a, "path_observed_at", at())
            .unwrap()
            .is_none()
    );

    // doc_b y doc_c se preservan con sus rutas intactas
    assert!(
        store
            .resolve(&doc_b, "path_observed_at", at())
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .resolve(&doc_c, "path_observed_at", at())
            .unwrap()
            .is_some()
    );
}
