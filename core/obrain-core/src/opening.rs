//! Reconocimiento previo a cualquier apertura con permisos de escritura.
//!
//! SQLite puede aceptar una imagen antigua válida sin las transacciones del
//! WAL. Integridad e identidad son comprobaciones distintas. Además, abrir en
//! READ_ONLY no impide que SQLite reconstruya SHM: inspeccionamos una copia
//! privada y desechable, nunca la única copia que se intenta recuperar.
use crate::{Error, Result, schema::SCHEMA_VERSION};
use obrain_keys::DataKey;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

pub(crate) fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

pub(crate) fn suspect(reason: impl Into<String>) -> Error {
    Error::UnrecognizedDatabase(reason.into())
}

/// Restos de una creación, restauración o conversión interrumpida impiden
/// interpretar una ruta ausente como una bóveda nueva.
pub(crate) fn has_artifacts(path: &Path, include_key: bool) -> bool {
    [
        "-wal",
        "-shm",
        "-journal",
        ".cifrando",
        ".cifrando-wal",
        ".cifrando-shm",
        ".cifrando-journal",
        ".llave.tmp",
    ]
    .iter()
    .any(|s| suffix(path, s).exists())
        || (include_key && obrain_keys::key_path(path).exists())
}

pub(crate) fn key(conn: &Connection, key: Option<&DataKey>) -> Result<()> {
    if let Some(key) = key {
        conn.execute_batch(&format!(
            "PRAGMA key = \"{}\";",
            key.sqlcipher_literal().as_str()
        ))?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|_| Error::WrongKey)?;
    }
    conn.execute_batch("PRAGMA temp_store = MEMORY;")?;
    Ok(())
}

pub(crate) fn verify(path: &Path, data_key: Option<&DataKey>) -> Result<()> {
    if !path.is_file() || path.metadata()?.len() == 0 {
        return Err(suspect("falta el archivo de base o está vacío"));
    }
    let private = tempfile::Builder::new()
        .prefix("obrain-verify-")
        .tempdir()?;
    let copy = private.path().join("brain.obrain");
    std::fs::copy(path, &copy)?;
    // SHM es un índice reconstruible, no conocimiento. Un journal caliente
    // necesita recuperación: READ_ONLY lo rechazará sin tocar el origen.
    for extension in ["-wal", "-journal"] {
        let source = suffix(path, extension);
        match std::fs::copy(&source, suffix(&copy, extension)) {
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    let conn = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    key(&conn, data_key)?;
    validate(&conn)
}

pub(crate) fn validate(conn: &Connection) -> Result<()> {
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| {
            suspect(format!(
                "no se puede leer la versión; comprueba la base y su archivo .llave: {e}"
            ))
        })?;
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return Err(suspect(format!(
            "versión de esquema {version}; se admiten 1 a {SCHEMA_VERSION}"
        )));
    }
    // Columnas de la fuente de verdad compartidas por v1 y v2. No exigimos
    // proyecciones opcionales ni predicados que una migración legítima añade.
    for sql in [
        "SELECT id, created_at, sensitivity FROM entities LIMIT 0",
        "SELECT id, label, cardinality, symmetric, transitive, inverse, default_method FROM predicates LIMIT 0",
        "SELECT id, subject, predicate, object_kind, object_entity, object_text, object_number, valid_from, valid_to, asserted_at, retracted_at, method, source, confidence, evidence FROM assertions LIMIT 0",
    ] {
        conn.prepare(sql)
            .map_err(|e| suspect(format!("esquema Brain incompleto: {e}")))?;
    }
    let anchors: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE
         (type='table' AND name IN ('entities','predicates','assertions')) OR
         (type='trigger' AND tbl_name='assertions' AND name IN
          ('assertions_no_delete','assertions_immutable','assertions_retract_once')) OR
         (type='trigger' AND tbl_name='entities' AND name='entities_no_delete')",
        [],
        |r| r.get(0),
    )?;
    if anchors != 7 {
        return Err(suspect("faltan tablas o protecciones del log de Brain"));
    }
    if version == 2 {
        for sql in [
            "SELECT entity, state, origin, observed_hash, published_hash, backup_captured FROM content_publication LIMIT 0",
            "SELECT entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text FROM publication_backup_chunks LIMIT 0",
        ] {
            conn.prepare(sql)
                .map_err(|e| suspect(format!("esquema v2 incompleto: {e}")))?;
        }
    }
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(suspect(format!("quick_check: {check}")));
    }
    if conn.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(suspect("hay referencias inválidas entre tablas"));
    }
    Ok(())
}
