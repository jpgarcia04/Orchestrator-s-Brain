//! Escribir candidatos en el log de aserciones.
//!
//! Dos reglas gobiernan este módulo:
//!
//! 1. **Un archivo ya visto se reconoce por su identidad, no por su ruta**
//!    (invariante I8). Mover un archivo añade una observación de ruta; no
//!    crea una entidad nueva.
//! 2. **Sólo se escribe lo que ha cambiado.** El log es append-only y sólo
//!    crece (ADR-0001): reafirmar en cada escaneo los mismos 200.000 tamaños
//!    de archivo lo haría crecer sin aportar información.

use crate::scan::Candidate;
use crate::triage::Availability;
use obrain_core::{EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};

#[derive(Debug, Default)]
pub struct IngestReport {
    pub entities_created: usize,
    pub entities_recognised: usize,
    pub assertions_written: usize,
    /// Aserciones omitidas por no aportar nada nuevo. Es la métrica que
    /// mantiene el log proporcionado a la información real.
    pub assertions_unchanged: usize,
    pub errors: Vec<String>,
}

/// Escribe un lote. El llamante debe envolverlo en una transacción.
pub fn ingest(
    store: &Store,
    candidates: &[Candidate],
    at: i64,
    report: &mut IngestReport,
) -> obrain_core::Result<()> {
    let tp = TimePoint::at(at);

    for c in candidates {
        // ── 1 · ¿conocemos ya este archivo? ──
        let id_hex = c.identity.map(|i| i.to_hex());
        let existing = match &id_hex {
            Some(h) => store.find_subject_by_value("file_id", h, tp)?,
            None => None,
        };

        let entity = match existing {
            Some(e) => {
                report.entities_recognised += 1;
                e
            }
            None => {
                let e = store.create_entity(at)?;
                report.entities_created += 1;
                write(store, report, &e, "has_facet", Facet::Addressable.as_str(), at)?;
                if let Some(h) = &id_hex {
                    write(store, report, &e, "file_id", h.as_str(), at)?;
                }
                e
            }
        };

        // ── 2 · observaciones ──
        let path = c.path.to_string_lossy().to_string();
        write_if_changed(store, report, &entity, "path_observed_at", Value::Text(path), tp, at)?;
        write_if_changed(store, report, &entity, "file_size", Value::Number(c.size as f64), tp, at)?;
        write_if_changed(
            store, report, &entity, "modified_at",
            Value::Timestamp(c.modified_ms), tp, at,
        )?;
        write_if_changed(
            store, report, &entity, "availability",
            Value::Text(c.availability.as_str().to_string()), tp, at,
        )?;
        if let Some(h) = &c.hash {
            write_if_changed(store, report, &entity, "has_hash", Value::Text(h.clone()), tp, at)?;
        }
        if let Some(m) = mime_guess(&c.path) {
            write_if_changed(store, report, &entity, "mime_guess", Value::Text(m.to_string()), tp, at)?;
        }
    }
    Ok(())
}

fn write(
    store: &Store,
    report: &mut IngestReport,
    subject: &EntityId,
    predicate: &str,
    value: &str,
    at: i64,
) -> obrain_core::Result<()> {
    store.assert(
        NewAssertion::new(subject.clone(), predicate, value, Method::Deterministic, at)
            .source("scan@m1"),
    )?;
    report.assertions_written += 1;
    Ok(())
}

/// Escribe sólo si el valor vigente es distinto del nuevo.
///
/// Es la compactación mínima que exige ADR-0001: sin ella, un escaneo diario
/// sobre 200.000 archivos añadiría un millón de aserciones idénticas al mes.
fn write_if_changed(
    store: &Store,
    report: &mut IngestReport,
    subject: &EntityId,
    predicate: &str,
    value: Value,
    tp: TimePoint,
    at: i64,
) -> obrain_core::Result<()> {
    if let Some(current) = store.resolve(subject, predicate, tp)? {
        if current.object == value {
            report.assertions_unchanged += 1;
            return Ok(());
        }
    }
    store.assert(
        NewAssertion::new(subject.clone(), predicate, value, Method::Deterministic, at)
            .source("scan@m1"),
    )?;
    report.assertions_written += 1;
    Ok(())
}

/// Tipo estimado por extensión. Deliberadamente pobre: en M2 lo sustituye el
/// sniffing por contenido. Aquí sólo sirve para priorizar la cola de extracción.
fn mime_guess(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "pdf" => "application/pdf",
        "doc" | "docx" => "application/msword",
        "xls" | "xlsx" => "application/vnd.ms-excel",
        "ppt" | "pptx" => "application/vnd.ms-powerpoint",
        "txt" | "md" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "csv" => "text/csv",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "heic" => "image/heic",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" | "mov" => "video/mp4",
        "mp3" | "m4a" => "audio/mpeg",
        "wav" => "audio/wav",
        "zip" => "application/zip",
        _ => return None,
    })
}

/// Etiqueta legible para el informe.
pub fn availability_label(a: Availability) -> &'static str {
    match a {
        Availability::Local => "local (bytes en disco)",
        Availability::CloudDehydrated => "nube deshidratado (no se leyo)",
        Availability::CloudPlaceholder => "nube placeholder (NO se abrio)",
        Availability::Offline => "offline",
    }
}
