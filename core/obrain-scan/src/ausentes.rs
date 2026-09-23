//! **Lo que ya no está** ([ADR-0025]).
//!
//! Hasta aquí el escáner apuntaba lo que veía y nunca lo que dejaba de ver: un
//! documento borrado seguía en todas partes. Este módulo es la otra mitad del
//! recorrido: lo que el log cree que hay en una biblioteca, el recorrido no
//! vio **y el disco confirma que no existe** pasa a la papelera.
//!
//! Pasar a la papelera es **retractar** su ruta y su carpeta —el log sigue
//! sabiendo dónde estuvo y hasta cuándo (I4)— y afirmar `missing_since`. Lo que
//! se leyó de él (texto, vectores) se olvida después, en `obrain-api`, porque
//! son proyecciones de otros crates.
//!
//! [ADR-0025]: ../../../docs/adr/0025-lo-que-ya-no-esta.md

use obrain_core::{EntityId, Method, NewAssertion, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// «Ya no está desde»: cuándo se enteró el sistema de que el archivo ya no
/// existe.
pub const MISSING: &str = "missing_since";
/// «Olvidado el»: cuándo se borró lo que se había leído del documento.
pub const FORGOTTEN: &str = "forgotten_at";

const SOURCE: &str = "scan@m1";

/// ¿Está en la papelera, o ya olvidado? Es decir: ¿se sabe que ya no existe?
pub fn is_missing(store: &Store, e: &EntityId, tp: TimePoint) -> obrain_core::Result<bool> {
    Ok(store.resolve(e, MISSING, tp)?.is_some())
}

/// Todo lo que ya no está, de una vez: para quien filtra listas enteras.
pub fn missing_set(store: &Store, tp: TimePoint) -> obrain_core::Result<HashSet<EntityId>> {
    Ok(store
        .query(None, Some(MISSING), tp)?
        .into_iter()
        .map(|a| a.subject)
        .collect())
}

/// Pasa a la papelera lo de esta biblioteca que el recorrido no ha visto y el
/// disco dice que **no existe**. Devuelve cuántos.
///
/// Dos seguros, porque retractar de más es peor que tardar en retractar:
///
/// 1. **La biblioteca tiene que estar.** Si su carpeta no responde —un USB que
///    se desconectó a media pasada— no se toca nada.
/// 2. **Sólo cuenta «no encontrado».** Un error de permisos, un disco que no
///    responde o cualquier otra cosa dejan el documento donde estaba. Y como
///    esto se pregunta después de recorrer y hashear, el hueco de milisegundos
///    de un guardado atómico ya se ha cerrado.
///
/// El llamante lo envuelve en la misma transacción que la ingesta.
pub fn mark_missing(
    store: &Store,
    root: &Path,
    seen: &[EntityId],
    at: i64,
) -> obrain_core::Result<usize> {
    let tp = TimePoint::at(at);
    let raiz = crate::library_path(root);
    let vistos: HashSet<&EntityId> = seen.iter().collect();

    // La ruta vigente de cada entidad: la más reciente, como en todas partes.
    let mut rutas: HashMap<EntityId, String> = HashMap::new();
    for a in store.query(None, Some("path_observed_at"), tp)? {
        if let Some(t) = a.object.as_text() {
            rutas.entry(a.subject).or_insert_with(|| t.to_string());
        }
    }

    let mut n = 0;
    for (e, ruta) in rutas {
        if vistos.contains(&e) || !obrain_core::sensitivity::path_within(&ruta, &raiz) {
            continue;
        }
        if !root.is_dir() {
            break;
        }
        match std::fs::symlink_metadata(&ruta) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            _ => continue,
        }
        forget_location(store, &e, at)?;
        store.assert(
            NewAssertion::new(
                e.clone(),
                MISSING,
                Value::Timestamp(at),
                Method::Deterministic,
                at,
            )
            .source(SOURCE),
        )?;
        n += 1;
    }
    Ok(n)
}

/// Retracta dónde estaba: todas sus rutas y su carpeta. Lo escrito por el
/// usuario no se toca (I5); hoy nadie más escribe esto, pero el día que haya
/// regiones del usuario (§10 del maestro) una carpeta elegida a mano no puede
/// irse con el archivo.
fn forget_location(store: &Store, e: &EntityId, at: i64) -> obrain_core::Result<()> {
    let tp = TimePoint::at(at);
    for pred in ["path_observed_at", "contained_by"] {
        for a in store.query(Some(e), Some(pred), tp)? {
            if a.method != Method::User {
                store.retract(&a.id, at)?;
            }
        }
    }
    Ok(())
}

/// Un documento de la papelera ha vuelto: se retractan su `missing_since` y su
/// `forgotten_at`. Devuelve `true` si estaba fuera.
///
/// No hay que devolverle nada más: su ruta y su carpeta las vuelven a escribir
/// la ingesta y el enlazador, como a cualquier archivo, y lo que se sabía de él
/// —su fecha, sus correcciones— nunca se fue del log.
pub fn bring_back(store: &Store, e: &EntityId, at: i64) -> obrain_core::Result<bool> {
    let tp = TimePoint::at(at);
    let mut volvio = false;
    for pred in [MISSING, FORGOTTEN] {
        for a in store.query(Some(e), Some(pred), tp)? {
            store.retract(&a.id, at)?;
            volvio |= pred == MISSING;
        }
    }
    Ok(volvio)
}
