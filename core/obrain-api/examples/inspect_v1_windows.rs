//! Comprobaciones de una base sintética después de arrancar Tauri.

use obrain_api::Brain;
use obrain_core::{EntityId, TimePoint};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = std::env::args()
        .nth(1)
        .ok_or("uso: inspect_v1_windows <base>")?;
    let db_path = Path::new(&db).canonicalize()?;
    let temp_path = std::env::temp_dir().canonicalize()?;
    if !db_path.starts_with(temp_path)
        || !db_path.components().any(|part| {
            part.as_os_str()
                .to_string_lossy()
                .starts_with("obrain-v1-tauri-windows-")
        })
    {
        return Err("sólo se acepta la fixture sintética bajo TEMP".into());
    }
    let brain = Brain::open(&db_path)?;
    let conn = brain.store().conn();
    let good = EntityId("v1-verificado".into());
    let bad = EntityId("v1-cuarentena".into());
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let fk: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })?;
    let state = |id: &EntityId| -> obrain_core::rusqlite::Result<String> {
        conn.query_row(
            "SELECT state FROM content_publication WHERE entity=?1",
            [id.as_str()],
            |r| r.get(0),
        )
    };
    let good_state = state(&good)?;
    let bad_state = state(&bad)?;
    let good_hits = brain.search("faro", 10)?.hits;
    let bad_hits = brain.search("secreto-sintetico", 10)?.hits;
    let good_preview = brain.preview(&good, None);
    let bad_preview = brain.preview(&bad, None);
    let expiry =
        brain
            .store()
            .resolve(&good, "expires_on", TimePoint::at(obrain_scan::now_ms()))?;
    let (bad_extraction_status, bad_extraction_time): (String, i64) = conn.query_row(
        "SELECT status,extracted_at FROM extractions WHERE entity=?1",
        [bad.as_str()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let libraries = obrain_api::indexar::libraries(brain.store())?;
    let mut stmt =
        conn.prepare("SELECT entity, status, extracted_at FROM extractions ORDER BY entity")?;
    let extractions = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<obrain_core::rusqlite::Result<Vec<_>>>()?;
    let mut stmt =
        conn.prepare("SELECT entity, state, origin FROM content_publication ORDER BY entity")?;
    let publication = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<obrain_core::rusqlite::Result<Vec<_>>>()?;
    println!(
        "libraries={} extractions={extractions:?} publication={publication:?}",
        libraries.len()
    );
    println!(
        "version={version} integrity={integrity} fk={fk} good={good_state} bad={bad_state} good_hits={:?} bad_hits={:?} good_preview={:?} bad_preview={:?} bad_extraction_status={bad_extraction_status} bad_extracted_at={bad_extraction_time}",
        good_hits.iter().map(|h| &h.entity).collect::<Vec<_>>(),
        bad_hits.iter().map(|h| &h.entity).collect::<Vec<_>>(),
        good_preview.as_ref().map(|p| p.text.as_deref()),
        bad_preview.as_ref().map(|p| p.text.as_deref())
    );
    assert_eq!(version, 2);
    assert_eq!(libraries.len(), 1);
    assert_eq!(integrity, "ok");
    assert_eq!(fk, 0);
    assert_eq!(good_state, "ready");
    assert_eq!(bad_state, "pending");
    assert!(good_hits.iter().any(|h| h.entity == good.0));
    assert!(bad_hits.is_empty());
    assert!(
        good_preview?
            .text
            .as_deref()
            .is_some_and(|t| t.contains("faro"))
    );
    assert!(bad_preview.is_err());
    assert!(expiry.is_none());
    assert_eq!(bad_extraction_status, "failed");
    assert!(bad_extraction_time > 1500, "el hilo de fondo no reindexó");
    Ok(())
}
