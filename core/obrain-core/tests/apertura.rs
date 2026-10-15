//! Apertura real de SQLCipher: una imagen antigua sin su WAL no es una base nueva.
use obrain_core::{Store, rusqlite::Connection, schema};
use obrain_keys::KdfParams;
use std::path::{Path, PathBuf};

fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(suffix);
    p.into()
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|f| {
            let f = f.unwrap();
            (
                f.file_name().to_string_lossy().into_owned(),
                std::fs::read(f.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn sin_wal_necesario_se_rechaza_sin_escribir() {
    let dir = std::env::temp_dir().join(format!(
        "obrain-open-regression-{}",
        obrain_core::EntityId::new()
    ));
    std::fs::create_dir(&dir).unwrap();
    let seed = dir.join("seed.obrain");
    let created = obrain_keys::create_with(&seed, KdfParams::FAST_FOR_TESTS).unwrap();
    // El mismo bootstrap de SQLCipher sirve antes y después de la corrección;
    // el flujo de creación del producto se prueba en apertura_segura.rs.
    let store = Connection::open(&seed).unwrap();
    store
        .execute_batch(&format!(
            "PRAGMA key=\"{}\";",
            created.key.sqlcipher_literal().as_str()
        ))
        .unwrap();
    store.execute_batch("PRAGMA wal_autocheckpoint=0").unwrap();
    store.execute_batch(schema::DDL).unwrap();
    store
        .pragma_update(None, "user_version", schema::SCHEMA_VERSION)
        .unwrap();
    store.execute_batch("INSERT INTO entities(id,created_at) VALUES('sintetica',1);
        INSERT INTO predicates(id,label,cardinality,default_method) VALUES('about','tema','many','user');
        INSERT INTO assertions(id,subject,predicate,object_kind,object_text,asserted_at,method,confidence)
        VALUES('a','sintetica','about','text','conocimiento sintético',1,'user',1)").unwrap();
    let restore = dir.join("restore");
    std::fs::create_dir(&restore).unwrap();
    let db = restore.join("brain.obrain");
    std::fs::copy(&seed, &db).unwrap();
    std::fs::copy(suffix(&seed, ".llave"), suffix(&db, ".llave")).unwrap();
    let before = snapshot(&restore);
    assert!(
        Store::open(&db).is_err(),
        "no debe inicializar como nueva una imagen existente sin esquema"
    );
    assert_eq!(
        before,
        snapshot(&restore),
        "el rechazo debe conservar todos los bytes y nombres"
    );
    drop(store);
    std::fs::remove_dir_all(dir).unwrap();
}
