//! Publicación de una versión después de extracción y clasificación.

use obrain_core::{EntityId, Store, TimePoint};
use obrain_extract::{FileRow, Format};
use std::collections::HashSet;

/// No se publica una extracción fallida ni una proyección de otra versión.
/// Los formatos que no producen texto se clasifican con las señales de nombre
/// y carpeta existentes; no deben conservar chunks de la versión anterior.
fn eligible(store: &Store, file: &FileRow, ocr_available: bool) -> crate::Result<bool> {
    let textual = obrain_extract::format::detect(&file.path);
    let requires_extraction = textual.is_some_and(|format| {
        file.may_read_content()
            && file.size <= obrain_extract::MAX_FILE_BYTES
            && (format != Format::Image || ocr_available)
    });
    let previous = obrain_extract::index::previous(
        store.conn(),
        &file.entity,
        obrain_extract::chunk::CHUNKER,
        obrain_extract::chunk::CHUNKER_VERSION,
    )?;
    if requires_extraction {
        let Some(previous) = previous else {
            return Ok(false);
        };
        let same_format = textual.is_some_and(|f| {
            previous.extractor == f.as_str()
                || (f == Format::Pdf && previous.extractor == "pdf+ocr")
        });
        return Ok(same_format
            && previous.extractor_ver == textual.unwrap().version()
            && previous.file_hash.is_some()
            && previous.file_hash == file.file_hash
            && matches!(
                previous.status.as_str(),
                "ok" | "empty" | "binary" | "minified" | "wordlist"
            ));
    }
    // No se generó texto en esta pasada. Si quedaran fragmentos de una
    // extensión anterior, mostrarlos con la nueva clasificación sería falso.
    let chunks: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE entity=?1",
            [file.entity.as_str()],
            |r| r.get(0),
        )
        .map_err(obrain_core::Error::from)?;
    Ok(chunks == 0 && previous.as_ref().is_none_or(|p| p.status != "failed"))
}

fn pending_ids(store: &Store, origin: Option<&str>) -> obrain_core::Result<HashSet<EntityId>> {
    let mut stmt = store.conn().prepare(
        "SELECT entity FROM content_publication
          WHERE state='pending' AND (?1 IS NULL OR origin=?1)",
    )?;
    let rows = stmt.query_map([origin], |r| Ok(EntityId(r.get(0)?)))?;
    Ok(rows.collect::<obrain_core::rusqlite::Result<_>>()?)
}

/// Debe llamarse en la misma transacción que la clasificación. SQLite impide
/// que otro escritor cambie el hash entre `eligible` y `publish_content`.
pub(crate) fn publish_pending(
    store: &Store,
    files: &[FileRow],
    ocr_available: bool,
    origin: Option<&str>,
) -> crate::Result<usize> {
    let pending = pending_ids(store, origin)?;
    let mut published = 0;
    for file in files {
        if !pending.contains(&file.entity) || !eligible(store, file, ocr_available)? {
            continue;
        }
        if store.publish_content(&file.entity, file.file_hash.as_deref())? {
            published += 1;
        }
    }
    Ok(published)
}

/// Una base anterior se abrió con sus archivos en cuarentena. Reproyecta la
/// sensibilidad desde los chunks de esa base y libera sólo las versiones cuya
/// extracción coincide con el hash observado. No abre archivos del disco.
pub(crate) fn bootstrap_legacy(store: &mut Store) -> crate::Result<()> {
    if !store.legacy_publication_pending()? {
        return Ok(());
    }
    let now = obrain_scan::now_ms();
    store.transaction(|s| -> crate::Result<()> {
        let files = obrain_extract::load_files(s, TimePoint::at(now))?;
        let mut inside = obrain_link::dentro::DentroReport::default();
        obrain_link::dentro::link(s, now, &mut inside)?;
        let mut report = obrain_guard::GuardReport::default();
        obrain_guard::classify_all(s, &files, now, false, &mut report)?;
        let ocr_available = obrain_ocr::platform().is_some();
        publish_pending(s, &files, ocr_available, Some("legacy"))?;
        Ok(())
    })
}
