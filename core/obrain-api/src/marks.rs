//! Las carpetas que el usuario marcó como sensibles, vistas desde la interfaz.
//!
//! La regla está en `obrain-core` ([`Store::resolve_sensitivity`]) y la
//! escritura en `obrain-guard` ([`obrain_guard::mark_folder`]). Aquí sólo se
//! dice **en palabras de persona** qué carpeta protege qué, que es lo que
//! necesitan tres sitios de la interfaz: la cabecera de una carpeta (¿está
//! marcada, o lo está una de fuera?), la vista previa (¿por qué es sensible
//! esto?) y la vista «Sensibles» (¿qué carpetas marqué?). Ver [ADR-0020].
//!
//! [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md

use obrain_core::sensitivity::path_within;
use obrain_core::{EntityId, FolderMark, FolderMarks, Origin, Result, Store, TimePoint};

/// La carpeta marcada que protege algo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Protection {
    /// La carpeta marcada.
    pub folder: String,
    pub label: String,
    /// `sensitive` | `vault`.
    pub level: &'static str,
    /// La marca es de esta misma carpeta, no de una que la contiene. Sólo tiene
    /// sentido en la cabecera de una carpeta: una marca propia se puede quitar
    /// desde ahí; una heredada, sólo desde la carpeta que la lleva.
    pub own: bool,
}

/// Una carpeta marcada, para la lista de la vista «Sensibles».
#[derive(Debug, Clone, serde::Serialize)]
pub struct MarkedFolder {
    pub id: String,
    pub label: String,
    pub path: String,
    pub level: &'static str,
    /// Documentos que tiene dentro **ahora**, según su ruta de hoy.
    ///
    /// Cero no es un error, pero se dice: casi siempre es una carpeta que se
    /// borró o se movió a otro disco, y la marca se quedó en la ruta vieja
    /// protegiendo nada. Renombrarla o moverla dentro del mismo disco ya no la
    /// vacía: la carpeta sigue a su `FileID` (ADR-0027).
    pub documents: usize,
    /// Desde cuándo, epoch ms.
    pub since: i64,
}

fn protection(store: &Store, m: &FolderMark, own: bool, at: TimePoint) -> Protection {
    Protection {
        folder: m.folder.0.clone(),
        label: obrain_link::label(store, &m.folder, at),
        level: m.level.as_str(),
        own,
    }
}

/// Qué carpeta marcada **decide** el nivel de esta entidad, si alguna lo hace.
///
/// No basta con que una carpeta marcada la contenga: si el clasificador ya
/// dice bóveda, o si el usuario hizo una excepción después, la carpeta no es
/// el porqué. Por eso se resuelve con la misma regla que escribe la columna.
pub fn deciding(store: &Store, id: &EntityId, at: TimePoint) -> Result<Option<Protection>> {
    let marks = store.folder_marks(at)?;
    if marks.is_empty() {
        return Ok(None);
    }
    Ok(match store.resolve_sensitivity(id, at, &marks)?.origin {
        Origin::Folder(m) => {
            let own = m.folder == *id;
            Some(protection(store, &m, own, at))
        }
        _ => None,
    })
}

/// La marca que afecta a una carpeta: la suya propia si la tiene, y si no, la
/// más fuerte de las carpetas que la contienen.
pub fn of_folder(
    store: &Store,
    marks: &FolderMarks,
    folder: &EntityId,
    at: TimePoint,
) -> Result<Option<Protection>> {
    if let Some(m) = marks.of(folder) {
        return Ok(Some(protection(store, m, true, at)));
    }
    let Some(ruta) = store.location(folder, at)? else {
        return Ok(None);
    };
    Ok(marks
        .covering(&ruta)
        .max_by(|a, b| a.level.cmp(&b.level).then(a.path.len().cmp(&b.path.len())))
        .map(|m| protection(store, m, false, at)))
}

/// Las carpetas marcadas, con cuántos documentos protegen hoy.
pub fn marked(store: &Store, at: TimePoint) -> Result<Vec<MarkedFolder>> {
    let marks = store.folder_marks(at)?;
    if marks.is_empty() {
        return Ok(Vec::new());
    }
    // La ruta vigente de cada archivo: la primera por sujeto es la más
    // reciente (`query` ordena por precedencia y luego por fecha).
    let mut rutas: std::collections::HashMap<EntityId, String> = std::collections::HashMap::new();
    for a in store.query(None, Some("path_observed_at"), at)? {
        if let Some(t) = a.object.as_text() {
            rutas.entry(a.subject).or_insert_with(|| t.to_string());
        }
    }
    let mut out: Vec<MarkedFolder> = marks
        .iter()
        .map(|m| MarkedFolder {
            id: m.folder.0.clone(),
            label: obrain_link::label(store, &m.folder, at),
            documents: rutas.values().filter(|r| path_within(r, &m.path)).count(),
            path: m.path.clone(),
            level: m.level.as_str(),
            since: m.asserted_at,
        })
        .collect();
    out.sort_by(|a, b| {
        a.label
            .to_lowercase()
            .cmp(&b.label.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(out)
}

/// La carpeta que el log conoce con esta ruta, sin distinguir mayúsculas ni
/// el separador final. Es lo que usa `obrain sensible <carpeta>`: en la
/// terminal se escribe la ruta, no un identificador.
pub fn folder_at(store: &Store, path: &str, at: TimePoint) -> Result<Option<EntityId>> {
    let limpia = path.trim_end_matches(['\\', '/']);
    if let Some(e) = store.find_subject_by_value("folder_path", limpia, at)? {
        return Ok(Some(e));
    }
    for a in store.query(None, Some("folder_path"), at)? {
        if let Some(t) = a.object.as_text() {
            if path_within(t, limpia) && path_within(limpia, t) {
                return Ok(Some(a.subject));
            }
        }
    }
    Ok(None)
}
