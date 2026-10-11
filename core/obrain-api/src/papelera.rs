//! **La papelera** ([ADR-0025]): lo que ya no está en tu bóveda, durante 30
//! días, y el olvido.
//!
//! Qué pasa a la papelera lo decide el escáner (`obrain_scan::ausentes`): un
//! documento de una biblioteca que el disco confirma que ya no existe. Aquí se
//! decide lo demás: enseñarlo, y **olvidar** —a los [`PAPELERA_DIAS`] o cuando
//! lo pidas— lo que se leyó de él.
//!
//! Olvidar borra **proyecciones** (ADR-0005): el texto, sus fragmentos del
//! índice de palabras, sus vectores y el registro de lectura. Es lo que pesa.
//! El log no se toca (I4): qué era, dónde estuvo, cuándo caducaba y tus
//! correcciones se quedan, y si el archivo vuelve, vuelve con todo.
//!
//! [ADR-0025]: ../../../docs/adr/0025-lo-que-ya-no-esta.md

use obrain_core::{EntityId, Method, NewAssertion, Store, TimePoint, Value};
use obrain_scan::ausentes::{FORGOTTEN, MISSING};

/// Cuántos días se queda algo en la papelera antes de olvidarse.
pub const PAPELERA_DIAS: i64 = 30;

const DIA_MS: i64 = 86_400_000;

/// Un documento de la papelera, listo para enseñarse.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnPapelera {
    pub id: String,
    /// Su nombre, el de la última ruta donde estuvo.
    pub label: String,
    /// La última ruta donde estuvo.
    pub path: String,
    /// El nombre de la carpeta donde estaba.
    pub folder: Option<String>,
    pub sensitivity: &'static str,
    /// Cuándo se enteró obrain de que ya no estaba, en epoch ms.
    pub since: i64,
    /// Días hasta que se olvide. `0` es «hoy».
    pub days_left: i64,
}

/// ¿Está en la papelera? Ya no está **y** no se ha olvidado todavía.
fn en_papelera(store: &Store, e: &EntityId, at: TimePoint) -> obrain_core::Result<Option<i64>> {
    let Some(a) = store.resolve(e, MISSING, at)? else {
        return Ok(None);
    };
    if store.resolve(e, FORGOTTEN, at)?.is_some() {
        return Ok(None);
    }
    Ok(match a.object {
        Value::Timestamp(t) => Some(t),
        _ => Some(a.asserted_at),
    })
}

fn dias_restantes(since: i64, now: i64) -> i64 {
    let fin = since + PAPELERA_DIAS * DIA_MS;
    ((fin - now).max(0) + DIA_MS - 1) / DIA_MS
}

/// Si está en la papelera, lo que se enseña de él. Lo usa la ficha de la vista
/// previa para decir «Ya no está en tu bóveda».
pub fn of(store: &Store, e: &EntityId, at: TimePoint) -> obrain_core::Result<Option<EnPapelera>> {
    let Some(since) = en_papelera(store, e, at)? else {
        return Ok(None);
    };
    let path = store
        .resolve_last(e, "path_observed_at", at.known)?
        .and_then(|a| a.object.as_text().map(str::to_string))
        .unwrap_or_default();
    let folder = match store.resolve_last(e, "contained_by", at.known)? {
        Some(a) => a
            .object
            .as_entity()
            .map(|f| obrain_link::label(store, f, at)),
        None => None,
    };
    Ok(Some(EnPapelera {
        id: e.0.clone(),
        label: obrain_link::label(store, e, at),
        path,
        folder,
        sensitivity: store.sensitivity(e).map(|s| s.as_str()).unwrap_or("vault"),
        since,
        days_left: dias_restantes(since, at.known),
    }))
}

/// Todo lo que hay en la papelera, lo más reciente arriba.
pub fn list(store: &Store, at: TimePoint) -> obrain_core::Result<Vec<EnPapelera>> {
    let mut out = Vec::new();
    for a in store.query(None, Some(MISSING), at)? {
        if let Some(p) = of(store, &a.subject, at)? {
            out.push(p);
        }
    }
    out.sort_by(|a, b| b.since.cmp(&a.since).then_with(|| a.id.cmp(&b.id)));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

/// **Olvida** lo que se leyó de un documento de la papelera —o de todos, con
/// `None`—. Devuelve cuántos.
///
/// `method` dice quién lo pidió: [`Method::User`] si fuiste tú («Olvidar ya»,
/// «Vaciar papelera»), [`Method::Deterministic`] si pasó el plazo.
///
/// Un documento que no está en la papelera no se olvida: esto no es una forma
/// de borrar el texto de lo que sigue en tu bóveda.
pub fn forget(
    store: &Store,
    e: Option<&EntityId>,
    method: Method,
    now: i64,
) -> crate::Result<usize> {
    let at = TimePoint::at(now);
    let objetivo: Vec<EntityId> = match e {
        Some(e) => vec![e.clone()],
        None => list(store, at)?
            .into_iter()
            .map(|p| EntityId(p.id))
            .collect(),
    };
    obrain_extract::index::migrate(store.conn())?;
    obrain_embed::index::migrate(store.conn())?;
    let mut n = 0;
    for e in objetivo {
        if en_papelera(store, &e, at)?.is_none() {
            continue;
        }
        purge(store, &e)?;
        store.assert(
            NewAssertion::new(e, FORGOTTEN, Value::Timestamp(now), method, now).source(
                if method == Method::User {
                    "user@ui"
                } else {
                    "papelera@v1"
                },
            ),
        )?;
        n += 1;
    }
    Ok(n)
}

/// Olvida lo que lleva en la papelera [`PAPELERA_DIAS`] o más. Lo llama cada
/// pasada de indexado.
pub fn expire(store: &Store, now: i64) -> crate::Result<usize> {
    let at = TimePoint::at(now);
    let mut n = 0;
    for p in list(store, at)? {
        if now - p.since >= PAPELERA_DIAS * DIA_MS {
            n += forget(store, Some(&EntityId(p.id)), Method::Deterministic, now)?;
        }
    }
    Ok(n)
}

/// Borra las proyecciones de una entidad: vectores, fragmentos (y con ellos el
/// índice de palabras, por sus triggers) y el registro de lectura. Sin el
/// registro, si el archivo vuelve se lee otra vez desde cero.
fn purge(store: &Store, e: &EntityId) -> obrain_core::Result<()> {
    let c = store.conn();
    c.execute("DELETE FROM vectors WHERE entity = ?1", [e.as_str()])?;
    c.execute("DELETE FROM chunks WHERE entity = ?1", [e.as_str()])?;
    c.execute("DELETE FROM extractions WHERE entity = ?1", [e.as_str()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_dias_que_quedan_se_cuentan_hacia_arriba() {
        let t = 1_700_000_000_000;
        assert_eq!(dias_restantes(t, t), 30);
        assert_eq!(dias_restantes(t, t + 1), 30, "en cuanto pasa algo, aún 30");
        assert_eq!(dias_restantes(t, t + DIA_MS), 29);
        assert_eq!(dias_restantes(t, t + 30 * DIA_MS - 1), 1);
        assert_eq!(dias_restantes(t, t + 30 * DIA_MS), 0);
        assert_eq!(dias_restantes(t, t + 90 * DIA_MS), 0);
    }
}
