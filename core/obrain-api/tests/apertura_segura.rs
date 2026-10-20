//! Regresiones del flujo real de persistencia, sin MSI, documentos ni modelos.
use obrain_api::seguridad::{self, abrir_con};
use obrain_core::{EntityId, Method, NewAssertion, Store, rusqlite::Connection};
use obrain_extract::{chunk::Chunk, index};
use obrain_keys::KdfParams;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const K: KdfParams = KdfParams::FAST_FOR_TESTS;

fn suffix(p: &Path, s: &str) -> PathBuf {
    let mut value = p.as_os_str().to_owned();
    value.push(s);
    value.into()
}

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("obrain-safe-open-{}", EntityId::new()));
        std::fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn db(&self) -> PathBuf {
        self.root.join("brain.obrain")
    }
    fn copy(&self, db: &Path, wal: bool, shm: bool, key: bool) {
        for s in ["", "-wal", "-shm", ".llave"] {
            if (s == "-wal" && !wal) || (s == "-shm" && !shm) || (s == ".llave" && !key) {
                continue;
            }
            let source = suffix(db, s);
            if source.exists() {
                std::fs::copy(source, suffix(&self.db(), s)).unwrap();
            }
        }
    }
    fn files(&self) -> BTreeMap<String, blake3::Hash> {
        std::fs::read_dir(&self.root)
            .unwrap()
            .map(|f| {
                let f = f.unwrap();
                (
                    f.file_name().to_string_lossy().into_owned(),
                    blake3::hash(&std::fs::read(f.path()).unwrap()),
                )
            })
            .collect()
    }
    fn reject(&self) {
        let before = self.files();
        let error = abrir_con(&self.db(), K)
            .err()
            .expect("el estado sospechoso debe rechazarse");
        assert!(!error.to_string().is_empty());
        assert_eq!(before, self.files(), "el rechazo cambió archivos: {error}");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn populated(f: &Fixture) -> (Store, obrain_keys::Created) {
    let db = f.db();
    let key = obrain_keys::create_with(&db, K).unwrap();
    let s = Store::create_with(&db, Some(&key.key)).unwrap();
    s.conn()
        .execute_batch("PRAGMA wal_autocheckpoint=0")
        .unwrap();
    index::migrate(s.conn()).unwrap();
    let entities: Vec<_> = (0..15).map(|_| s.create_entity(1).unwrap()).collect();
    for i in 0..156 {
        s.assert(NewAssertion::new(
            entities[i % 15].clone(),
            "about",
            format!("dato sintético {i}"),
            Method::User,
            1,
        ))
        .unwrap();
    }
    for (i, entity) in entities.iter().take(8).enumerate() {
        let text = format!("pasaje sintético verificable {i}");
        index::replace_chunks(
            s.conn(),
            entity,
            "test",
            1,
            &[Chunk {
                ordinal: 0,
                byte_start: 0,
                byte_end: text.len(),
                text,
            }],
        )
        .unwrap();
    }
    (s, key)
}

fn contents(s: &Store) -> Vec<(String, String)> {
    s.conn()
        .prepare("SELECT id, object_text FROM assertions ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn intact(s: &Store, expected: &[(String, String)]) {
    for (table, count) in [("entities", 15), ("assertions", 156), ("chunks", 8)] {
        assert_eq!(
            s.conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            count
        );
    }
    assert_eq!(contents(s), expected);
    let texts: Vec<String> = s
        .conn()
        .prepare("SELECT text FROM chunks ORDER BY text")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        texts,
        (0..8)
            .map(|i| format!("pasaje sintético verificable {i}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        s.conn()
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        !s.conn()
            .prepare("PRAGMA cipher_integrity_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    assert!(
        !s.conn()
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    assert_eq!(
        s.conn()
            .query_row(
                "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'verificable'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        8
    );
}

#[test]
fn completa_con_wal_y_sin_shm_conserva_conocimiento() {
    let seed = Fixture::new();
    let (s, _) = populated(&seed);
    let expected = contents(&s);
    for shm in [true, false] {
        let restore = Fixture::new();
        restore.copy(&seed.db(), true, shm, true);
        let reopened = abrir_con(&restore.db(), K).unwrap();
        intact(&reopened, &expected);
        drop(reopened);
        intact(&abrir_con(&restore.db(), K).unwrap(), &expected);
    }
}

#[test]
fn perder_wal_necesario_no_inicializa_una_boveda_vacia() {
    let seed = Fixture::new();
    let (_s, _) = populated(&seed);
    let restore = Fixture::new();
    restore.copy(&seed.db(), false, false, true);
    restore.reject();
}

#[test]
fn checkpoint_permite_abrir_sin_wal() {
    let seed = Fixture::new();
    let (s, _) = populated(&seed);
    let expected = contents(&s);
    s.conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let restore = Fixture::new();
    restore.copy(&seed.db(), false, false, true);
    intact(&abrir_con(&restore.db(), K).unwrap(), &expected);
}

#[test]
fn llave_ausente_o_incorrecta_no_altera_archivos() {
    let seed = Fixture::new();
    let (_s, _) = populated(&seed);
    let missing = Fixture::new();
    missing.copy(&seed.db(), true, true, false);
    missing.reject();
    let wrong = Fixture::new();
    wrong.copy(&seed.db(), true, false, false);
    obrain_keys::create_with(&wrong.db(), K).unwrap();
    wrong.reject();
}

#[test]
fn llave_huerfana_no_crea_base() {
    let f = Fixture::new();
    obrain_keys::create_with(&f.db(), K).unwrap();
    f.reject();
    let before = f.files();
    assert!(Store::open(f.db()).is_err());
    assert_eq!(before, f.files());
}

#[test]
fn archivo_vacio_o_ajeno_no_se_cifra_ni_se_inicializa() {
    let empty = Fixture::new();
    std::fs::write(empty.db(), []).unwrap();
    empty.reject();
    let foreign = Fixture::new();
    {
        let c = Connection::open(foreign.db()).unwrap();
        c.execute_batch("CREATE TABLE other(x); PRAGMA user_version=1")
            .unwrap();
    }
    foreign.reject();
}

#[test]
fn versiones_desconocidas_y_esquemas_incompletos_se_conservan() {
    for damage in [
        "PRAGMA user_version=0",
        "PRAGMA user_version=3",
        "DROP TRIGGER assertions_immutable",
        "DROP TABLE publication_backup_chunks",
    ] {
        let f = Fixture::new();
        let (s, _) = populated(&f);
        s.conn().execute_batch(damage).unwrap();
        drop(s);
        f.reject();
    }
}

#[test]
fn recuperacion_no_reescribe_llave_de_base_incompleta_o_ausente() {
    let seed = Fixture::new();
    let (_s, key) = populated(&seed);
    for missing_db in [false, true] {
        let restore = Fixture::new();
        restore.copy(&seed.db(), false, false, true);
        if missing_db {
            std::fs::remove_file(restore.db()).unwrap();
        }
        let before = restore.files();
        assert!(seguridad::recuperar(&restore.db(), &key.recovery.to_string()).is_err());
        assert_eq!(before, restore.files());
    }
}

#[test]
fn recuperacion_correcta_conserva_datos_y_codigo_incorrecto_no_escribe() {
    let seed = Fixture::new();
    let (s, key) = populated(&seed);
    let expected = contents(&s);
    let restore = Fixture::new();
    restore.copy(&seed.db(), true, false, true);
    let before = restore.files();
    assert!(seguridad::recuperar(&restore.db(), "AAAAA-AAAAA-AAAAA-AAAAA-AAAAA").is_err());
    assert_eq!(before, restore.files());
    seguridad::recuperar(&restore.db(), &key.recovery.to_string()).unwrap();
    intact(&abrir_con(&restore.db(), K).unwrap(), &expected);
}

#[test]
fn restos_huerfanos_no_se_sobrescriben() {
    for rest in ["-wal", "-shm", "-journal", ".cifrando", ".llave.tmp"] {
        let f = Fixture::new();
        std::fs::write(suffix(&f.db(), rest), b"resto sintetico").unwrap();
        f.reject();
    }
}

#[test]
fn reanudacion_rechaza_copia_sospechosa_antes_de_sustituir() {
    let f = Fixture::new();
    let s = Store::open(f.db()).unwrap();
    let entity = s.create_entity(1).unwrap();
    s.assert(NewAssertion::new(
        entity,
        "about",
        "origen que no debe perderse",
        Method::User,
        1,
    ))
    .unwrap();
    drop(s);
    let key = obrain_keys::create_with(&f.db(), K).unwrap();
    {
        let c = Connection::open(suffix(&f.db(), ".cifrando")).unwrap();
        c.execute_batch(&format!(
            "PRAGMA key=\"{}\"; PRAGMA user_version=0;",
            key.key.sqlcipher_literal().as_str()
        ))
        .unwrap();
    }
    f.reject();
}

#[test]
fn dos_conexiones_validas_pueden_seguir_escribiendo() {
    let f = Fixture::new();
    let (s, _) = populated(&f);
    let expected = contents(&s);
    let other = abrir_con(&f.db(), K).unwrap();
    intact(&other, &expected);
    let entity = other.create_entity(2).unwrap();
    assert_eq!(
        s.conn()
            .query_row(
                "SELECT count(*) FROM entities WHERE id=?1",
                [&entity.0],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn v1_historica_reconocida_se_migra_y_se_cifra() {
    let f = Fixture::new();
    {
        let c = Connection::open(f.db()).unwrap();
        c.execute_batch(include_str!("fixtures/schema_v1.sql"))
            .unwrap();
        c.execute_batch("PRAGMA user_version=1;
            INSERT INTO entities(id,created_at) VALUES('legacy',1);
            INSERT INTO predicates(id,label,cardinality,default_method) VALUES('about','tema','many','user');
            INSERT INTO assertions(id,subject,predicate,object_kind,object_text,asserted_at,method,confidence)
            VALUES('a','legacy','about','text','conocimiento anterior',1,'user',1);").unwrap();
        index::migrate(&c).unwrap();
        index::replace_chunks(
            &c,
            &EntityId("legacy".into()),
            "test",
            1,
            &[Chunk {
                ordinal: 0,
                byte_start: 0,
                byte_end: 8,
                text: "anterior".into(),
            }],
        )
        .unwrap();
    }
    let migrated = abrir_con(&f.db(), K).unwrap();
    assert_eq!(
        contents(&migrated),
        vec![("a".into(), "conocimiento anterior".into())]
    );
    assert_eq!(
        migrated
            .conn()
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        migrated
            .conn()
            .query_row("SELECT text FROM chunks", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "anterior"
    );
    assert!(obrain_keys::exists(&f.db()));
    assert_eq!(
        migrated
            .conn()
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn conversion_interrumpida_reconocida_se_reanuda() {
    let f = Fixture::new();
    let original = Store::open(f.db()).unwrap();
    let entity = original.create_entity(1).unwrap();
    original
        .assert(NewAssertion::new(
            entity,
            "about",
            "antes del corte",
            Method::User,
            1,
        ))
        .unwrap();
    let expected = contents(&original);
    seguridad::migrate(original.conn()).unwrap();
    drop(original);
    let key = obrain_keys::prepare(K).unwrap();
    let pending = suffix(&f.db(), ".cifrando");
    Store::export_encrypted(f.db(), &pending, key.key()).unwrap();
    key.write(&f.db()).unwrap(); // Corte entre la publicación de la llave y rename.
    let reopened = abrir_con(&f.db(), K).unwrap();
    assert_eq!(contents(&reopened), expected);
    assert!(!pending.exists());
}

#[test]
fn copia_cifrando_no_sustituye_una_base_ya_cifrada() {
    let f = Fixture::new();
    let (s, _) = populated(&f);
    s.conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    std::fs::copy(f.db(), suffix(&f.db(), ".cifrando")).unwrap();
    drop(s);
    f.reject();
}

#[test]
fn referencias_rotas_se_rechazan_antes_de_migrar() {
    let f = Fixture::new();
    let (s, _) = populated(&f);
    s.conn().execute_batch("PRAGMA foreign_keys=OFF;
        INSERT INTO assertions(id,subject,predicate,object_kind,object_text,asserted_at,method,confidence)
        VALUES('rota','inexistente','about','text','dato',1,'user',1)").unwrap();
    drop(s);
    f.reject();
}

#[test]
fn checkpoint_ocupado_no_se_presenta_como_completo() {
    let f = Fixture::new();
    let (writer, _) = populated(&f);
    let reader = Store::open(f.db()).unwrap();
    reader
        .conn()
        .execute_batch("BEGIN; SELECT * FROM entities LIMIT 1")
        .unwrap();
    writer.create_entity(2).unwrap();
    writer
        .conn()
        .busy_timeout(std::time::Duration::from_millis(1))
        .unwrap();
    assert!(Store::checkpoint(writer.conn()).is_err());
    reader.conn().execute_batch("ROLLBACK").unwrap();
    Store::checkpoint(writer.conn()).unwrap();
}

#[test]
fn corrupcion_de_pagina_no_modifica_la_copia_rechazada() {
    let f = Fixture::new();
    let (s, _) = populated(&f);
    drop(s);
    let mut bytes = std::fs::read(f.db()).unwrap();
    assert!(bytes.len() > 3 * 4096);
    bytes[2 * 4096 + 128] ^= 0xff;
    std::fs::write(f.db(), bytes).unwrap();
    f.reject();
}

#[test]
fn exportacion_no_toca_un_origen_ajeno_ni_crea_destino() {
    let f = Fixture::new();
    {
        let c = Connection::open(f.db()).unwrap();
        c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE other(x); PRAGMA user_version=1")
            .unwrap();
    }
    let pending = obrain_keys::prepare(K).unwrap();
    let before = f.files();
    assert!(Store::export_encrypted(f.db(), suffix(&f.db(), ".cifrando"), pending.key()).is_err());
    assert_eq!(before, f.files());
}
