// Bases construidas con el DDL v1 de 683e63b y proyecciones de extracción v1.
// Representan un corte entre extracción y vencimientos, sin partir de v2.

use obrain_core::rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub const OLD_DATE: &str = "2025-11-15";
pub const DOC: &str = "legacy-doc";

pub struct Legacy {
    root: PathBuf,
    db: PathBuf,
}

impl Drop for Legacy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Copy)]
pub struct Document<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub text: &'a str,
    pub old_date: Option<&'a str>,
    pub manual_date: Option<&'a str>,
    pub manual_sensitivity: Option<&'a str>,
    pub extraction_verified: bool,
}

impl Legacy {
    pub fn new(label: &str, documents: &[Document<'_>]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "obrain-v1-{label}-{}-{}-{}",
            std::process::id(),
            obrain_scan::now_ms(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let library = root.join("Biblioteca");
        std::fs::create_dir_all(&library).unwrap();
        let db = root.join("brain.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(include_str!("schema_v1.sql")).unwrap();
            conn.execute_batch(obrain_extract::index::DDL).unwrap();
            conn.execute_batch("PRAGMA user_version=1").unwrap();
            conn.execute(
                "INSERT INTO entities(id, created_at) VALUES('legacy-folder', 1)",
                [],
            )
            .unwrap();
            for predicate in [
                "folder_path",
                "has_facet",
                "path_observed_at",
                "contained_by",
                "availability",
                "has_hash",
                "file_size",
                "mime_guess",
                "expires_on",
                "sensitivity_level",
            ] {
                let cardinality = if predicate == "has_facet" {
                    "many"
                } else {
                    "one"
                };
                conn.execute(
                    "INSERT INTO predicates(id,label,cardinality,default_method)
                     VALUES(?1,?1,?2,'deterministic')",
                    params![predicate, cardinality],
                )
                .unwrap();
            }
            text_assertion(
                &conn,
                "folder-path",
                "legacy-folder",
                "folder_path",
                &library.to_string_lossy(),
                "deterministic",
                "scan@v1",
            );
            text_assertion(
                &conn,
                "folder-facet",
                "legacy-folder",
                "has_facet",
                "library",
                "deterministic",
                "scan@v1",
            );
            for d in documents {
                let file = library.join(d.name);
                std::fs::write(&file, d.text).unwrap();
                let hash = blake3::hash(d.text.as_bytes()).to_hex().to_string();
                conn.execute(
                    "INSERT INTO entities(id,created_at,sensitivity) VALUES(?1,1,'vault')",
                    [d.id],
                )
                .unwrap();
                text_assertion(
                    &conn,
                    &format!("{}-path", d.id),
                    d.id,
                    "path_observed_at",
                    &file.to_string_lossy(),
                    "deterministic",
                    "scan@v1",
                );
                text_assertion(
                    &conn,
                    &format!("{}-hash", d.id),
                    d.id,
                    "has_hash",
                    &hash,
                    "deterministic",
                    "scan@v1",
                );
                text_assertion(
                    &conn,
                    &format!("{}-availability", d.id),
                    d.id,
                    "availability",
                    "local",
                    "deterministic",
                    "scan@v1",
                );
                text_assertion(
                    &conn,
                    &format!("{}-mime", d.id),
                    d.id,
                    "mime_guess",
                    "text/plain",
                    "deterministic",
                    "extract@v1",
                );
                conn.execute(
                    "INSERT INTO assertions(id,subject,predicate,object_kind,object_number,
                                            asserted_at,method,source,confidence)
                     VALUES(?1,?2,'file_size','number',?3,1000,'deterministic','scan@v1',1)",
                    params![format!("{}-size", d.id), d.id, d.text.len() as f64],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO assertions(id,subject,predicate,object_kind,object_entity,
                                            asserted_at,method,source,confidence)
                     VALUES(?1,?2,'contained_by','entity','legacy-folder',1000,
                            'deterministic','link@v1',1)",
                    params![format!("{}-home", d.id), d.id],
                )
                .unwrap();
                if let Some(date) = d.old_date {
                    text_assertion(
                        &conn,
                        &format!("{}-old-expiry", d.id),
                        d.id,
                        "expires_on",
                        date,
                        "inferred",
                        "expiry@v2",
                    );
                }
                if let Some(date) = d.manual_date {
                    text_assertion(
                        &conn,
                        &format!("{}-manual-expiry", d.id),
                        d.id,
                        "expires_on",
                        date,
                        "user",
                        "user@v1",
                    );
                }
                if let Some(level) = d.manual_sensitivity {
                    text_assertion(
                        &conn,
                        &format!("{}-manual-sensitivity", d.id),
                        d.id,
                        "sensitivity_level",
                        level,
                        "user",
                        "user@v1",
                    );
                }
                conn.execute(
                    "INSERT INTO extractions(entity,extractor,extractor_ver,chunker,chunker_ver,
                                             status,file_hash,text_hash,chars,chunk_count,extracted_at)
                     VALUES(?1,'plaintext',1,'paragraph-pack',1,'ok',?2,'synthetic',?3,1,1500)",
                    params![d.id, if d.extraction_verified { hash.as_str() } else { "mismatch" }, d.text.chars().count() as i64],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO chunks(entity,chunker,chunker_ver,ordinal,byte_start,byte_end,text)
                     VALUES(?1,'paragraph-pack',1,0,0,?2,?3)",
                    params![d.id, d.text.len() as i64, d.text],
                )
                .unwrap();
            }
        }
        Self { root, db }
    }

    pub fn path(&self) -> &Path {
        &self.db
    }
}

fn text_assertion(
    conn: &Connection,
    id: &str,
    subject: &str,
    predicate: &str,
    text: &str,
    method: &str,
    source: &str,
) {
    conn.execute(
        "INSERT INTO assertions(id,subject,predicate,object_kind,object_text,
                                asserted_at,method,source,confidence)
         VALUES(?1,?2,?3,'text',?4,1000,?5,?6,1)",
        params![id, subject, predicate, text, method, source],
    )
    .unwrap();
}

pub fn doc(text: &'static str) -> Document<'static> {
    Document {
        id: DOC,
        name: "expediente.txt",
        text,
        old_date: Some(OLD_DATE),
        manual_date: None,
        manual_sensitivity: None,
        extraction_verified: true,
    }
}
