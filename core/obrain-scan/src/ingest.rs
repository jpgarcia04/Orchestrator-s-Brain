//! Escribir candidatos en el log de aserciones.
//!
//! Tres reglas gobiernan este módulo:
//!
//! 1. **Un archivo ya visto se reconoce por su identidad, no por su ruta**
//!    (invariante I8). Mover un archivo añade una observación de ruta; no
//!    crea una entidad nueva.
//! 2. **Cuando la identidad se rompe, la ruta la rescata.** El guardado
//!    atómico cambia el `FileID` de NTFS, así que I8 por sí solo ve un archivo
//!    nuevo. Ver [`reconcile`].
//! 3. **Sólo se escribe lo que ha cambiado.** El log es append-only y sólo
//!    crece (ADR-0001): reafirmar en cada escaneo los mismos 200.000 tamaños
//!    de archivo lo haría crecer sin aportar información.

use crate::scan::Candidate;
use crate::triage::Availability;
use obrain_core::{EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct IngestReport {
    pub entities_created: usize,
    pub entities_recognised: usize,
    /// Reconocidas por ruta tras un cambio de `FileID`. Cada una es una
    /// entidad fantasma que **no** se ha creado.
    pub entities_reconciled: usize,
    pub assertions_written: usize,
    /// Aserciones omitidas por no aportar nada nuevo. Es la métrica que
    /// mantiene el log proporcionado a la información real.
    pub assertions_unchanged: usize,
    /// `file_id` caducados que se han retractado al reconciliar, y rutas y
    /// carpetas viejas de lo que se movió o ya no está.
    pub assertions_retracted: usize,
    /// Documentos que estaban en la papelera (o ya olvidados) y han vuelto: por
    /// su `FileID` o por su contenido (ADR-0025).
    pub entities_returned: usize,
    /// Las entidades que este recorrido ha visto, para saber después qué **no**
    /// ha visto ([`crate::ausentes::mark_missing`]).
    pub seen: Vec<EntityId>,
    /// Rutas cuya identidad no permite una decisión segura. No se publican.
    pub deferred: Vec<String>,
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

    // Todas las identidades de este recorrido, antes de escribir nada. Es lo
    // que permite a `reconcile` distinguir "el archivo se guardó" de "el
    // archivo se movió y otro ocupó su sitio": si el `FileID` viejo sigue
    // apareciendo en algún sitio, el archivo viejo sigue existiendo.
    let seen: HashSet<String> = candidates
        .iter()
        .filter_map(|c| c.identity.map(|i| i.to_hex()))
        .collect();
    let mut id_counts: HashMap<String, usize> = HashMap::new();
    let mut hash_counts: HashMap<&str, usize> = HashMap::new();
    for c in candidates {
        if let Some(id) = c.identity {
            *id_counts.entry(id.to_hex()).or_default() += 1;
        }
        if let Some(hash) = &c.hash {
            *hash_counts.entry(hash).or_default() += 1;
        }
    }
    // Una sola observación sin FileID puede ser el archivo anterior movido.
    // No basta con que el resto del inventario sea completo.
    let complete_identity = candidates.iter().all(|c| c.identity.is_some());

    for c in candidates {
        let path = c.path.to_string_lossy().to_string();
        let Some(identity) = c.identity else {
            defer(store, report, &path, tp, at)?;
            continue;
        };
        let id_hex = Some(identity.to_hex());
        if id_counts[identity.to_hex().as_str()] != 1 {
            defer(store, report, &path, tp, at)?;
            continue;
        }
        // ── 1 · ¿conocemos ya este archivo? ──
        let owners = store.find_subjects_by_value("file_id", id_hex.as_deref().unwrap(), tp)?;
        if owners.len() > 1 {
            defer(store, report, &path, tp, at)?;
            continue;
        }
        let existing = owners.into_iter().next();

        let entity = match existing {
            Some(e) => {
                report.entities_recognised += 1;
                e
            }
            None => match if complete_identity {
                reconcile(store, &seen, c, tp)?
            } else {
                None
            } {
                Some(e) => {
                    report.entities_reconciled += 1;
                    // El `file_id` viejo apuntaba a algo que ya no existe.
                    // Dejarlo vivo sería una contradicción de cardinalidad
                    // `one` y haría que un escaneo futuro volviera a
                    // reconciliar en vez de reconocer.
                    if let Some(a) = store.resolve(&e, "file_id", tp)? {
                        store.retract(&a.id, at)?;
                        report.assertions_retracted += 1;
                    }
                    if let Some(h) = &id_hex {
                        write(store, report, &e, "file_id", h.as_str(), at)?;
                    }
                    e
                }
                None if path_is_ambiguous(store, &path, &seen, tp)? => {
                    defer(store, report, &path, tp, at)?;
                    continue;
                }
                None => match if complete_identity
                    && c.hash.as_deref().is_some_and(|h| hash_counts[h] == 1)
                {
                    returning(store, c, tp)?
                } else {
                    None
                } {
                    // Un documento de la papelera, con el mismo contenido y
                    // otro `FileID`: lo copiaste de una copia de seguridad.
                    Some(e) => {
                        if let Some(a) = store.resolve(&e, "file_id", tp)? {
                            store.retract(&a.id, at)?;
                            report.assertions_retracted += 1;
                        }
                        if let Some(h) = &id_hex {
                            write(store, report, &e, "file_id", h.as_str(), at)?;
                        }
                        e
                    }
                    None => {
                        let e = store.create_entity(at)?;
                        report.entities_created += 1;
                        write(
                            store,
                            report,
                            &e,
                            "has_facet",
                            Facet::Addressable.as_str(),
                            at,
                        )?;
                        if let Some(h) = &id_hex {
                            write(store, report, &e, "file_id", h.as_str(), at)?;
                        }
                        e
                    }
                },
            },
        };
        // La ruta puede cambiar de regla de nombre/carpeta aunque el hash sea
        // igual. Sin hash (archivo grande), tamaño y mtime son la señal de
        // cambio disponible. La preparación comparte el commit de la ingesta:
        // ningún lector ve la nueva observación aún publicada como `normal`.
        let before_path = store
            .resolve(&entity, "path_observed_at", tp)?
            .and_then(|a| a.object.as_text().map(str::to_string));
        let before_hash = store
            .resolve(&entity, "has_hash", tp)?
            .and_then(|a| a.object.as_text().map(str::to_string));
        let changed = before_path.as_deref() != Some(path.as_str())
            || match &c.hash {
                Some(hash) => before_hash.as_deref() != Some(hash.as_str()),
                None => {
                    let old_size =
                        store
                            .resolve(&entity, "file_size", tp)?
                            .and_then(|a| match a.object {
                                Value::Number(n) => Some(n as u64),
                                _ => None,
                            });
                    let old_mtime =
                        store
                            .resolve(&entity, "modified_at", tp)?
                            .and_then(|a| match a.object {
                                Value::Timestamp(t) => Some(t),
                                _ => None,
                            });
                    old_size != Some(c.size) || old_mtime != Some(c.modified_ms)
                }
            };
        if changed {
            store.prepare_content(&entity, c.hash.as_deref())?;
        }
        for pending in store.query(Some(&entity), Some("identity_pending_path"), tp)? {
            store.retract(&pending.id, at)?;
            report.assertions_retracted += 1;
        }
        // Una observación con identidad fiable resuelve también las pistas
        // pendientes de otros dueños de esta ruta: ya no son su localización.
        for other in store.find_subjects_by_value("identity_pending_path", &path, tp)? {
            if other == entity {
                continue;
            }
            for pending in store.query(Some(&other), Some("identity_pending_path"), tp)? {
                if pending.object.as_text() == Some(path.as_str()) {
                    store.retract(&pending.id, at)?;
                    report.assertions_retracted += 1;
                }
            }
        }

        // ── 1b · ¿volvía de la papelera? ──
        if crate::ausentes::bring_back(store, &entity, at)? {
            report.entities_returned += 1;
        }
        report.seen.push(entity.clone());

        // ── 2 · observaciones ──
        write_if_changed(
            store,
            report,
            &entity,
            "path_observed_at",
            Value::Text(path.clone()),
            tp,
            at,
        )?;
        // Las rutas de antes dejan de estar vigentes: el archivo ya no está
        // ahí. Sin esto, un archivo movido seguía «observado» en su sitio
        // viejo, y quien leía todas las rutas lo enseñaba en los dos
        // (ADR-0025). Retractar y no borrar (I4): el log sigue sabiendo dónde
        // estuvo y hasta cuándo.
        for a in store.query(Some(&entity), Some("path_observed_at"), tp)? {
            if a.object.as_text() != Some(path.as_str()) {
                store.retract(&a.id, at)?;
                report.assertions_retracted += 1;
            }
        }
        write_if_changed(
            store,
            report,
            &entity,
            "file_size",
            Value::Number(c.size as f64),
            tp,
            at,
        )?;
        write_if_changed(
            store,
            report,
            &entity,
            "modified_at",
            Value::Timestamp(c.modified_ms),
            tp,
            at,
        )?;
        write_if_changed(
            store,
            report,
            &entity,
            "availability",
            Value::Text(c.availability.as_str().to_string()),
            tp,
            at,
        )?;
        if let Some(h) = &c.hash {
            write_if_changed(
                store,
                report,
                &entity,
                "has_hash",
                Value::Text(h.clone()),
                tp,
                at,
            )?;
        }
        if let Some(m) = mime_guess(&c.path) {
            write_if_changed(
                store,
                report,
                &entity,
                "mime_guess",
                Value::Text(m.to_string()),
                tp,
                at,
            )?;
        }
    }
    Ok(())
}

fn path_is_ambiguous(
    store: &Store,
    path: &str,
    seen: &HashSet<String>,
    tp: TimePoint,
) -> obrain_core::Result<bool> {
    let claimants = path_claimants(store, path, tp)?;
    if claimants.len() > 1 {
        return Ok(true);
    }
    for e in claimants {
        let old = store.resolve(&e, "file_id", tp)?;
        if !old
            .and_then(|a| a.object.as_text().map(|s| seen.contains(s)))
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn path_claimants(store: &Store, path: &str, tp: TimePoint) -> obrain_core::Result<Vec<EntityId>> {
    let mut claimants = store.find_subjects_by_value("path_observed_at", path, tp)?;
    claimants.extend(store.find_subjects_by_value("identity_pending_path", path, tp)?);
    claimants.sort();
    claimants.dedup();
    Ok(claimants)
}

/// Retira sólo la localización discutida; conserva la entidad, correcciones,
/// relaciones y log. Sin ruta vigente no entra en los cargadores de documentos.
fn defer(
    store: &Store,
    report: &mut IngestReport,
    path: &str,
    tp: TimePoint,
    at: i64,
) -> obrain_core::Result<()> {
    report.deferred.push(path.to_string());
    for e in store.find_subjects_by_value("path_observed_at", path, tp)? {
        if !store.has_assertion(
            &e,
            "identity_pending_path",
            &Value::Text(path.to_string()),
            tp,
        )? {
            write(store, report, &e, "identity_pending_path", path, at)?;
        }
        for a in store.query(Some(&e), Some("path_observed_at"), tp)? {
            if a.object.as_text() == Some(path) && a.method != Method::User {
                store.retract(&a.id, at)?;
                report.assertions_retracted += 1;
            }
        }
    }
    Ok(())
}

/// La entidad que ya ocupaba esta ruta, si el archivo de antes ha dejado de
/// existir. `None` significa "esto es un archivo nuevo de verdad".
///
/// ## El problema
///
/// El invariante I8 dice que la identidad es `(volumen, FileID) + hash` y que
/// sobrevive a mover, renombrar y desconectar discos. Sobrevive a las tres.
/// **No sobrevive al guardado atómico**: un editor que escribe en un temporal
/// y renombra encima deja un `FileID` distinto, y así es como guardan casi
/// todos los editores modernos y las aplicaciones de Office. El escaneo
/// siguiente ve un archivo nuevo, crea una entidad nueva, y la vieja se queda
/// con la misma ruta, el contenido caducado y sus fragmentos.
///
/// Para la puerta de sensibilidad el fallo era hacia el lado seguro. Para el
/// índice vectorial no lo es: son dos juegos de vectores del mismo documento,
/// y la búsqueda devuelve el archivo dos veces, una de ellas con texto que ya
/// no existe en el disco.
///
/// ## La regla
///
/// Se reconcilia cuando se cumplen las tres a la vez:
///
/// 1. El `FileID` de este candidato **no se conocía**.
/// 2. Su ruta la reclama **exactamente una** entidad viva.
/// 3. El `FileID` de esa entidad **no aparece en ningún sitio de este
///    recorrido**.
///
/// La tercera es la que hace la regla determinista y no una heurística. Si el
/// archivo viejo siguiera existiendo —movido a otra carpeta— su `FileID`
/// estaría entre los candidatos, y entonces el archivo que hay ahora en la
/// ruta antigua es genuinamente otro. Sin esa comprobación, mover un archivo y
/// crear otro con su nombre fusionaría dos documentos sin relación, que es un
/// error mucho peor que el que se está arreglando.
///
/// ## Lo que la regla no exige, a propósito
///
/// **No exige que el hash coincida.** El caso que destapó el problema fue
/// editar `search.rs` para redactar un NIE: cambiaron a la vez el contenido y
/// el `FileID`. Exigir el hash habría dejado fuera justo ese caso — el más
/// común, porque nadie guarda un archivo sin cambiarlo. El hash entra en el
/// informe como matiz (¿mismo contenido o edición?), no como condición.
fn reconcile(
    store: &Store,
    seen: &HashSet<String>,
    c: &Candidate,
    tp: TimePoint,
) -> obrain_core::Result<Option<EntityId>> {
    let path = c.path.to_string_lossy().to_string();
    let claimants = path_claimants(store, &path, tp)?;
    let [old] = claimants.as_slice() else {
        return Ok(None);
    };
    // ¿Sigue vivo el archivo que era esa entidad?
    let Some(a) = store.resolve(old, "file_id", tp)? else {
        return Ok(None);
    };
    let Some(old_id) = a.object.as_text() else {
        return Ok(None);
    };
    if seen.contains(old_id) {
        return Ok(None);
    }
    Ok(Some(old.clone()))
}

/// El documento de la papelera que vuelve con **el mismo contenido**: el único
/// que no está en el disco y tiene este hash (ADR-0025, §5).
///
/// Sólo entre lo que ya no está, y sólo si es uno: si dos documentos borrados
/// tenían el mismo contenido, no hay forma de saber cuál de los dos es éste, y
/// un documento nuevo es mejor que heredar las correcciones del equivocado.
fn returning(store: &Store, c: &Candidate, tp: TimePoint) -> obrain_core::Result<Option<EntityId>> {
    let Some(h) = &c.hash else {
        return Ok(None);
    };
    let mut gone = Vec::new();
    for e in store.find_subjects_by_value("has_hash", h, tp)? {
        if crate::ausentes::is_missing(store, &e, tp)? {
            gone.push(e);
        }
    }
    Ok(match <[EntityId; 1]>::try_from(gone) {
        Ok([e]) => Some(e),
        Err(_) => None,
    })
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

/// La ruta de una biblioteca **como la escriben los enlazadores**: sin
/// separador final.
///
/// M3 identifica cada carpeta por la cadena que queda a la izquierda del último
/// separador de la ruta de un archivo, así que `C:\Bóveda\` y `C:\Bóveda`
/// serían dos carpetas distintas para él. Se recorta aquí para que la
/// biblioteca y la carpeta que ve el enlazador sean la misma entidad.
pub fn library_path(root: &std::path::Path) -> String {
    let s = root.to_string_lossy();
    let t = s.trim_end_matches(['\\', '/']);
    if t.is_empty() {
        s.into_owned()
    } else {
        t.to_string()
    }
}

/// Marca `root` como **biblioteca**: una carpeta que el usuario eligió indexar.
///
/// Es la entidad de carpeta de siempre —la misma que crearía M3 para esa
/// ruta— con la faceta `library` encima. Si ya existía, se reutiliza: una base
/// antigua no gana una segunda carpeta con la misma ruta.
///
/// La faceta la afirma el **usuario** y no el escáner: elegir qué carpeta es
/// tu bóveda es una decisión tuya (I5), no una observación del disco.
///
/// Idempotente. El llamante envuelve en transacción.
pub fn register_library(
    store: &Store,
    root: &std::path::Path,
    at: i64,
) -> obrain_core::Result<EntityId> {
    let path = library_path(root);
    let tp = TimePoint::at(at);
    let e = match store.find_subject_by_value("folder_path", &path, tp)? {
        Some(e) => e,
        None => {
            let e = store.create_entity(at)?;
            store.assert(
                NewAssertion::new(
                    e.clone(),
                    "folder_path",
                    path.as_str(),
                    Method::Deterministic,
                    at,
                )
                .source("scan:biblioteca"),
            )?;
            e
        }
    };
    for (facet, method) in [
        (Facet::Container, Method::Deterministic),
        (Facet::Library, Method::User),
    ] {
        let v = Value::Text(facet.as_str().to_string());
        if !store.has_assertion(&e, "has_facet", &v, tp)? {
            store.assert(
                NewAssertion::new(e.clone(), "has_facet", facet.as_str(), method, at)
                    .source("scan:biblioteca"),
            )?;
        }
    }
    Ok(e)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::FileIdentity;
    use std::path::PathBuf;

    /// Los dos `FileID` reales que dejó el caso que destapó el problema:
    /// editar `search.rs` para redactar un NIE y volver a escanear.
    const ID_ANTES: [u8; 16] = [0xb6, 0x70, 0, 0, 0, 0, 0, 0x55, 0x01, 0, 0, 0, 0, 0, 0, 0];
    const ID_DESPUES: [u8; 16] = [0xe3, 0x32, 0x02, 0, 0, 0, 0, 0x22, 0, 0, 0, 0, 0, 0, 0, 0];
    const RUTA: &str = r"C:\Users\usuario\OneDrive\Documentos\GitHub\proyecto\src\search.rs";

    fn candidato(path: &str, file_id: [u8; 16], hash: &str) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            availability: Availability::Local,
            size: 4096,
            modified_ms: 1_700_000_000_000,
            identity: Some(FileIdentity {
                volume: 0x7838_c906_38c8_c3fa,
                file_id,
            }),
            hash: Some(hash.to_string()),
        }
    }

    fn rutas_con_varias_entidades(store: &Store) -> i64 {
        store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM (
                     SELECT object_text FROM assertions
                      WHERE predicate = 'path_observed_at' AND retracted_at IS NULL
                      GROUP BY object_text HAVING COUNT(DISTINCT subject) > 1)",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// **El caso real.** Guardar con un editor que escribe en temporal y
    /// renombra cambia el `FileID`, y el contenido también cambió. Antes de
    /// esta regla el resultado eran dos entidades vivas con la misma ruta:
    /// dos juegos de vectores del mismo documento.
    #[test]
    fn el_guardado_atomico_no_crea_una_entidad_fantasma() {
        let store = Store::open_in_memory().unwrap();
        let mut r = IngestReport::default();

        ingest(
            &store,
            &[candidato(RUTA, ID_ANTES, "hash-con-nie")],
            1_000,
            &mut r,
        )
        .unwrap();
        assert_eq!(r.entities_created, 1);

        // Se edita el archivo y se vuelve a escanear.
        let mut r2 = IngestReport::default();
        ingest(
            &store,
            &[candidato(RUTA, ID_DESPUES, "hash-redactado")],
            2_000,
            &mut r2,
        )
        .unwrap();

        assert_eq!(r2.entities_created, 0, "no es un archivo nuevo");
        assert_eq!(r2.entities_reconciled, 1);
        assert_eq!(
            r2.assertions_retracted, 1,
            "el file_id viejo deja de estar vigente"
        );

        let n: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "un documento, una entidad");
        assert_eq!(rutas_con_varias_entidades(&store), 0);
    }

    /// Y a la tercera pasada ya se reconoce por identidad, sin reconciliar:
    /// la reconciliación arregla el log, no se queda viviendo en él.
    #[test]
    fn tras_reconciliar_el_archivo_vuelve_a_reconocerse_por_identidad() {
        let store = Store::open_in_memory().unwrap();
        let mut r = IngestReport::default();
        ingest(&store, &[candidato(RUTA, ID_ANTES, "a")], 1_000, &mut r).unwrap();
        ingest(&store, &[candidato(RUTA, ID_DESPUES, "b")], 2_000, &mut r).unwrap();

        let mut r3 = IngestReport::default();
        ingest(&store, &[candidato(RUTA, ID_DESPUES, "b")], 3_000, &mut r3).unwrap();
        assert_eq!(r3.entities_recognised, 1);
        assert_eq!(r3.entities_reconciled, 0);
        assert_eq!(r3.entities_created, 0);
        assert_eq!(
            r3.assertions_written, 0,
            "nada que decir, nada que escribir"
        );
    }

    /// El error caro que la tercera condición evita: mover un archivo y poner
    /// otro con su nombre **no** son el mismo documento. Aquí el `FileID`
    /// viejo sigue apareciendo en el recorrido, así que no se reconcilia.
    #[test]
    fn mover_un_archivo_y_ocupar_su_hueco_no_fusiona_dos_documentos() {
        let store = Store::open_in_memory().unwrap();
        let mut r = IngestReport::default();
        ingest(
            &store,
            &[candidato(RUTA, ID_ANTES, "el-de-siempre")],
            1_000,
            &mut r,
        )
        .unwrap();

        // El original se movió a otra carpeta y en su sitio hay otro archivo.
        let movido = candidato(r"C:\otra\carpeta\search.rs", ID_ANTES, "el-de-siempre");
        let intruso = candidato(RUTA, ID_DESPUES, "otro-contenido");

        let mut r2 = IngestReport::default();
        ingest(&store, &[movido, intruso], 2_000, &mut r2).unwrap();

        assert_eq!(
            r2.entities_reconciled, 0,
            "el archivo viejo sigue existiendo"
        );
        assert_eq!(r2.entities_created, 1, "el intruso es un documento nuevo");
        assert_eq!(
            r2.entities_recognised, 1,
            "el movido se reconoce por su FileID"
        );
    }

    /// Sin cambios en el disco no se escribe nada. Es la propiedad que M1 midió
    /// y que la reconciliación no puede romper.
    #[test]
    fn la_reconciliacion_no_rompe_la_idempotencia() {
        let store = Store::open_in_memory().unwrap();
        let c = [candidato(RUTA, ID_ANTES, "x")];
        let mut r = IngestReport::default();
        ingest(&store, &c, 1_000, &mut r).unwrap();

        let mut r2 = IngestReport::default();
        ingest(&store, &c, 2_000, &mut r2).unwrap();
        assert_eq!(r2.entities_created, 0);
        assert_eq!(r2.entities_reconciled, 0);
        assert_eq!(r2.assertions_written, 0);
        assert_eq!(r2.assertions_retracted, 0);
    }

    #[test]
    fn sin_file_id_se_aplaza_y_no_hereda_la_entidad_ni_se_publica() {
        let store = Store::open_in_memory().unwrap();
        let mut first = IngestReport::default();
        ingest(
            &store,
            &[candidato(RUTA, ID_ANTES, "original")],
            1_000,
            &mut first,
        )
        .unwrap();
        let original = first.seen[0].clone();
        store
            .assert(NewAssertion::new(
                original.clone(),
                "expires_on",
                "2030-05-01",
                Method::User,
                1_100,
            ))
            .unwrap();
        let before = store.log_len().unwrap();
        let mut unknown = candidato(RUTA, ID_DESPUES, "otro");
        unknown.identity = None;
        let mut second = IngestReport::default();
        ingest(&store, &[unknown], 2_000, &mut second).unwrap();
        assert_eq!(second.deferred, [RUTA]);
        assert!(second.seen.is_empty());
        assert!(
            store
                .find_subjects_by_value("path_observed_at", RUTA, TimePoint::at(2_000))
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .resolve(&original, "expires_on", TimePoint::at(2_000))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            store.log_len().unwrap(),
            before + 1,
            "la ruta pendiente deja rastro y el log original permanece"
        );
        assert!(
            store
                .resolve(&original, "path_observed_at", TimePoint::at(1_500))
                .unwrap()
                .is_some()
        );
        let mut third = IngestReport::default();
        ingest(
            &store,
            &[candidato(RUTA, ID_DESPUES, "otro")],
            3_000,
            &mut third,
        )
        .unwrap();
        assert_eq!(third.seen, [original]);
        assert!(third.deferred.is_empty());
        assert!(
            store
                .resolve(
                    &third.seen[0],
                    "identity_pending_path",
                    TimePoint::at(3_000)
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn un_file_id_en_dos_rutas_no_se_resuelve_por_orden() {
        let store = Store::open_in_memory().unwrap();
        let mut report = IngestReport::default();
        let one = candidato(r"C:\prueba\uno.txt", ID_ANTES, "igual");
        let two = candidato(r"C:\prueba\dos.txt", ID_ANTES, "igual");
        ingest(&store, &[one, two], 1_000, &mut report).unwrap();
        assert_eq!(report.deferred.len(), 2);
        assert_eq!(report.entities_created, 0);
        assert!(report.seen.is_empty());
        assert_eq!(store.log_len().unwrap(), 0);
    }

    /// La biblioteca es la carpeta de siempre con una faceta encima: registrar
    /// dos veces no escribe dos veces, y un separador final no crea otra.
    #[test]
    fn registrar_una_biblioteca_es_idempotente() {
        let store = Store::open_in_memory().unwrap();
        let a =
            register_library(&store, std::path::Path::new(r"C:\Users\jp\Bóveda\"), 1000).unwrap();
        let antes = store.log_len().unwrap();
        let b =
            register_library(&store, std::path::Path::new(r"C:\Users\jp\Bóveda"), 2000).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            store.log_len().unwrap(),
            antes,
            "la segunda vez no escribe nada"
        );

        let tp = TimePoint::at(3000);
        let ruta = store.resolve(&a, "folder_path", tp).unwrap().unwrap();
        assert_eq!(ruta.object.as_text(), Some(r"C:\Users\jp\Bóveda"));
        let facetas: Vec<String> = store
            .query(Some(&a), Some("has_facet"), tp)
            .unwrap()
            .into_iter()
            .filter_map(|x| x.object.as_text().map(str::to_string))
            .collect();
        assert!(facetas.contains(&"library".to_string()));
        assert!(facetas.contains(&"container".to_string()));
    }
}
