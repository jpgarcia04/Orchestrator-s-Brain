//! **Tus colecciones** ([ADR-0031]): lo que juntas a mano —«Trámite de la
//! visa»— venga de la carpeta que venga.
//!
//! Una colección es una entidad sin archivo, con tres clases de aserciones y
//! **todas del usuario** (I5), porque ningún enlazador las escribe:
//!
//! - `has_facet = region`: es una colección;
//! - `collection_name`: cómo se llama (cardinalidad `one`: renombrar retracta
//!   el nombre de antes);
//! - `member_of`, del documento a la colección: lo que tiene dentro. Sacar un
//!   documento retracta la suya; el log recuerda que estuvo (I4).
//!
//! Lo que **no** hace, y es la decisión: no cambia la sensibilidad de nada, no
//! mueve estrellas, no entra nada solo (ni lo que llegue a una carpeta ni lo
//! que lleve dentro un paquete) y no toca tus archivos.
//!
//! [ADR-0031]: ../../../docs/adr/0031-tus-colecciones.md

use crate::sky::Row;
use obrain_core::{EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

/// El nombre: cardinalidad `one`, del usuario.
pub const NAME: &str = "collection_name";
/// Un documento está en una colección. Del documento a la colección.
pub const MEMBER_OF: &str = "member_of";
/// Lo más largo que puede ser un nombre. Es una etiqueta de la barra lateral,
/// no una descripción.
pub const NOMBRE_MAX: usize = 80;

/// Por qué no se pudo hacer algo con una colección. Lo que dice cada uno es
/// lo que se le enseña a la persona.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("una colección necesita un nombre")]
    SinNombre,

    #[error("el nombre es demasiado largo: como mucho {NOMBRE_MAX} letras")]
    NombreLargo,

    #[error("ya tienes una colección que se llama «{0}»")]
    Repetida(String),

    #[error("esa colección ya no existe")]
    NoExiste,

    #[error("«{0}» ya no está en tu bóveda: sólo se juntan documentos que estén en ella")]
    NoEsDocumento(String),
}

/// Una colección, para la barra lateral y la vista «Colecciones».
#[derive(Debug, Clone, serde::Serialize)]
pub struct Coleccion {
    pub id: String,
    pub name: String,
    /// Documentos que tiene **en tu bóveda**: sin contar la papelera.
    pub documents: usize,
    /// De los suyos, cuántos están en la papelera (y no olvidados).
    pub in_trash: usize,
    /// Desde cuándo existe, epoch ms.
    pub since: i64,
}

/// Una colección a la que pertenece un documento: lo que enseña su vista
/// previa.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Pertenece {
    pub id: String,
    pub name: String,
}

/// Un documento de una colección abierta.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Miembro {
    #[serde(flatten)]
    pub row: Row,
    /// Su carpeta de verdad: en una colección cada uno viene de un sitio.
    pub folder: String,
    pub folder_label: String,
    /// El paquete de la colección bajo el que va, si también lo metiste
    /// (ADR-0030): la lista ya viene en ese orden.
    pub inside: Option<String>,
    /// Las carpetas por encima, de dentro hacia fuera: qué estrella del mapa
    /// encender.
    pub ancestors: Vec<String>,
}

/// Una colección abierta: lo que tiene dentro.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Abierta {
    pub id: String,
    pub name: String,
    pub since: i64,
    pub rows: Vec<Miembro>,
    /// En cuántas carpetas distintas están.
    pub folders: usize,
    /// Los suyos que están en la papelera, por si quieres ir a verlos.
    pub in_trash: Vec<crate::EnPapelera>,
}

fn region() -> Value {
    Value::Text(Facet::Region.as_str().to_string())
}

/// El nombre limpio: sin espacios de sobra. Error si queda vacío o largo.
fn limpio(nombre: &str) -> Result<String, Error> {
    let n = nombre.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() {
        return Err(Error::SinNombre);
    }
    if n.chars().count() > NOMBRE_MAX {
        return Err(Error::NombreLargo);
    }
    Ok(n)
}

/// ¿Es una colección viva? Su faceta la puso el usuario y no la retractó.
pub fn es(store: &Store, id: &EntityId, at: TimePoint) -> obrain_core::Result<bool> {
    Ok(store
        .query(Some(id), Some("has_facet"), at)?
        .iter()
        .any(|a| a.method == Method::User && a.object == region()))
}

fn nombre(store: &Store, id: &EntityId, at: TimePoint) -> obrain_core::Result<String> {
    Ok(store
        .resolve(id, NAME, at)?
        .and_then(|a| a.object.as_text().map(str::to_string))
        .unwrap_or_default())
}

/// Las colecciones vivas, con cuándo nacieron.
fn vivas(store: &Store, at: TimePoint) -> obrain_core::Result<Vec<(EntityId, i64)>> {
    let mut out: Vec<(EntityId, i64)> = store
        .query(None, Some("has_facet"), at)?
        .into_iter()
        .filter(|a| a.method == Method::User && a.object == region())
        .map(|a| (a.subject, a.asserted_at))
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    Ok(out)
}

/// Otra colección viva que ya se llama así, sin distinguir mayúsculas.
fn repetida(
    store: &Store,
    nombre: &str,
    salvo: Option<&EntityId>,
    at: TimePoint,
) -> obrain_core::Result<Option<String>> {
    let buscado = nombre.to_lowercase();
    for (c, _) in vivas(store, at)? {
        if Some(&c) == salvo {
            continue;
        }
        let n = self::nombre(store, &c, at)?;
        if n.to_lowercase() == buscado {
            return Ok(Some(n));
        }
    }
    Ok(None)
}

fn existe(store: &Store, id: &EntityId, at: TimePoint) -> crate::Result<()> {
    if es(store, id, at)? {
        Ok(())
    } else {
        Err(Error::NoExiste.into())
    }
}

fn del_usuario(
    subject: &EntityId,
    predicate: &str,
    object: Value,
    now: i64,
    source: &str,
) -> NewAssertion {
    NewAssertion::new(subject.clone(), predicate, object, Method::User, now).source(source)
}

/// **Crea una colección** vacía. `source` dice desde dónde (`user@ui`,
/// `user@cli`).
pub fn crear(store: &Store, nombre: &str, now: i64, source: &str) -> crate::Result<EntityId> {
    let at = TimePoint::at(now);
    let n = limpio(nombre)?;
    if let Some(otra) = repetida(store, &n, None, at)? {
        return Err(Error::Repetida(otra).into());
    }
    let id = store.create_entity(now)?;
    store.assert(del_usuario(&id, "has_facet", region(), now, source))?;
    store.assert(del_usuario(&id, NAME, Value::Text(n), now, source))?;
    Ok(id)
}

/// **Le cambia el nombre.** El de antes se retracta, no se borra.
pub fn renombrar(
    store: &Store,
    id: &EntityId,
    nombre: &str,
    now: i64,
    source: &str,
) -> crate::Result<()> {
    let at = TimePoint::at(now);
    existe(store, id, at)?;
    let n = limpio(nombre)?;
    if let Some(otra) = repetida(store, &n, Some(id), at)? {
        return Err(Error::Repetida(otra).into());
    }
    store.assert_by_user(del_usuario(id, NAME, Value::Text(n), now, source))?;
    Ok(())
}

/// Las `member_of` vivas que apuntan a esta colección.
fn pertenencias(
    store: &Store,
    id: &EntityId,
    at: TimePoint,
) -> obrain_core::Result<Vec<obrain_core::Assertion>> {
    Ok(store
        .pointing_at(id, Some(MEMBER_OF), at)?
        .into_iter()
        .filter(|a| a.method == Method::User)
        .collect())
}

/// **Borra una colección.** Se retractan su faceta, su nombre y lo que tenía
/// dentro; **tus documentos no se tocan** —ni tus archivos, ni sus carpetas,
/// ni las otras colecciones—. El log recuerda que existió (I4). Devuelve
/// cuántos documentos tenía.
pub fn borrar(store: &Store, id: &EntityId, now: i64) -> crate::Result<usize> {
    let at = TimePoint::at(now);
    existe(store, id, at)?;
    let dentro = pertenencias(store, id, at)?;
    for a in &dentro {
        store.retract(&a.id, now)?;
    }
    for a in store.query(Some(id), None, at)? {
        if a.method == Method::User
            && (a.predicate == NAME || (a.predicate == "has_facet" && a.object == region()))
        {
            store.retract(&a.id, now)?;
        }
    }
    let mut docs: Vec<&EntityId> = dentro.iter().map(|a| &a.subject).collect();
    docs.sort();
    docs.dedup();
    Ok(docs.len())
}

/// **Mete documentos** en una colección. Devuelve cuántos entraron (los que
/// ya estaban no cuentan). Sólo documentos de tu bóveda: algo con una ruta
/// vigente que no sea una carpeta. Nada entra a medias: si uno no vale, no
/// entra ninguno.
pub fn meter(
    store: &Store,
    id: &EntityId,
    docs: &[EntityId],
    now: i64,
    source: &str,
) -> crate::Result<usize> {
    let at = TimePoint::at(now);
    existe(store, id, at)?;
    for d in docs {
        // Una colección o una carpeta no tienen ruta de archivo: esto basta.
        let es_doc = store.resolve(d, "path_observed_at", at)?.is_some()
            && store.resolve(d, "folder_path", at)?.is_none();
        if !es_doc {
            return Err(Error::NoEsDocumento(obrain_link::label(store, d, at)).into());
        }
    }
    let ya: HashSet<EntityId> = pertenencias(store, id, at)?
        .into_iter()
        .map(|a| a.subject)
        .collect();
    let mut n = 0;
    let mut vistos = HashSet::new();
    for d in docs {
        if ya.contains(d) || !vistos.insert(d.clone()) {
            continue;
        }
        store.assert(del_usuario(
            d,
            MEMBER_OF,
            Value::Entity(id.clone()),
            now,
            source,
        ))?;
        n += 1;
    }
    Ok(n)
}

/// **Saca documentos** de una colección: se retracta su `member_of`. Los
/// documentos no se tocan. Devuelve cuántos salieron.
pub fn sacar(store: &Store, id: &EntityId, docs: &[EntityId], now: i64) -> crate::Result<usize> {
    let at = TimePoint::at(now);
    existe(store, id, at)?;
    let quitar: HashSet<&EntityId> = docs.iter().collect();
    let mut salieron = HashSet::new();
    for a in pertenencias(store, id, at)? {
        if quitar.contains(&a.subject) {
            store.retract(&a.id, now)?;
            salieron.insert(a.subject);
        }
    }
    Ok(salieron.len())
}

/// Los documentos de una colección, **todos**: los de la bóveda y los de la
/// papelera. Sin repetir, por identificador.
pub fn miembros(store: &Store, id: &EntityId, at: TimePoint) -> obrain_core::Result<Vec<EntityId>> {
    let mut v: Vec<EntityId> = pertenencias(store, id, at)?
        .into_iter()
        .map(|a| a.subject)
        .collect();
    v.sort();
    v.dedup();
    Ok(v)
}

/// Los documentos de una colección **que están en tu bóveda**: lo que se
/// busca cuando buscas dentro de ella.
pub fn en_la_boveda(
    store: &Store,
    id: &EntityId,
    at: TimePoint,
) -> obrain_core::Result<HashSet<EntityId>> {
    let fuera = obrain_scan::ausentes::missing_set(store, at)?;
    Ok(miembros(store, id, at)?
        .into_iter()
        .filter(|e| !fuera.contains(e))
        .collect())
}

/// Todas tus colecciones, por nombre.
pub fn todas(store: &Store, at: TimePoint) -> crate::Result<Vec<Coleccion>> {
    let fuera = obrain_scan::ausentes::missing_set(store, at)?;
    let mut out = Vec::new();
    for (c, since) in vivas(store, at)? {
        let mut documents = 0;
        let mut in_trash = 0;
        for m in miembros(store, &c, at)? {
            if !fuera.contains(&m) {
                documents += 1;
            } else if crate::papelera::of(store, &m, at)?.is_some() {
                in_trash += 1;
            }
        }
        out.push(Coleccion {
            name: nombre(store, &c, at)?,
            id: c.0,
            documents,
            in_trash,
            since,
        });
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}

/// Las colecciones vivas donde está un documento, por nombre.
pub fn de(store: &Store, doc: &EntityId, at: TimePoint) -> obrain_core::Result<Vec<Pertenece>> {
    let mut out = Vec::new();
    for a in store.query(Some(doc), Some(MEMBER_OF), at)? {
        let Some(c) = a.object.as_entity() else {
            continue;
        };
        if a.method != Method::User
            || !es(store, c, at)?
            || out.iter().any(|p: &Pertenece| p.id == c.0)
        {
            continue;
        }
        out.push(Pertenece {
            id: c.0.clone(),
            name: nombre(store, c, at)?,
        });
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

/// **Abre una colección**: lo que tiene en tu bóveda, por nombre, con cada
/// documento de dentro de un paquete justo debajo de él si también lo
/// metiste (ADR-0030), y lo suyo que está en la papelera aparte.
pub fn abrir(store: &Store, id: &EntityId, at: TimePoint) -> crate::Result<Abierta> {
    existe(store, id, at)?;
    let fuera = obrain_scan::ausentes::missing_set(store, at)?;
    let todos = miembros(store, id, at)?;
    let (vivos, idos): (Vec<EntityId>, Vec<EntityId>) =
        todos.into_iter().partition(|e| !fuera.contains(e));

    let mut sueltos = crate::sky::loose(store, &vivos, at)?;
    sueltos.sort_by(|a, b| {
        a.row
            .label
            .to_lowercase()
            .cmp(&b.row.label.to_lowercase())
            .then_with(|| a.row.id.cmp(&b.row.id))
    });

    // Lo de dentro, debajo de su paquete, si los dos están aquí.
    let ids: Vec<String> = sueltos.iter().map(|s| s.row.id.clone()).collect();
    let aqui: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut paquetes: HashMap<String, Vec<String>> = HashMap::new();
    for s in &sueltos {
        let de: Vec<String> = store
            .query(
                Some(&EntityId(s.row.id.clone())),
                Some(obrain_core::PART_OF),
                at,
            )?
            .into_iter()
            .filter_map(|a| a.object.as_entity().map(|e| e.0.clone()))
            .filter(|p| aqui.contains(p.as_str()))
            .collect();
        if !de.is_empty() {
            paquetes.insert(s.row.id.clone(), de);
        }
    }
    let orden = crate::dentro::agrupar(&ids, &paquetes);
    let mut viejos: Vec<Option<crate::sky::Suelto>> = sueltos.into_iter().map(Some).collect();
    let rows: Vec<Miembro> = orden
        .into_iter()
        .filter_map(|(i, inside)| {
            viejos[i].take().map(|s| Miembro {
                folder: s.folder.0,
                folder_label: s.folder.1,
                row: s.row,
                inside,
                ancestors: s.ancestors,
            })
        })
        .collect();

    let carpetas: HashSet<&str> = rows.iter().map(|m| m.folder.as_str()).collect();
    let mut in_trash = Vec::new();
    for e in idos {
        if let Some(p) = crate::papelera::of(store, &e, at)? {
            in_trash.push(p);
        }
    }
    in_trash.sort_by_key(|p| p.label.to_lowercase());

    let since = vivas(store, at)?
        .into_iter()
        .find(|(c, _)| c == id)
        .map_or(0, |(_, t)| t);
    Ok(Abierta {
        id: id.0.clone(),
        name: nombre(store, id, at)?,
        since,
        folders: carpetas.len(),
        rows,
        in_trash,
    })
}

/// La colección viva que se llama así, sin distinguir mayúsculas: lo que usa
/// la terminal, donde se escribe el nombre y no un identificador.
pub fn por_nombre(
    store: &Store,
    nombre: &str,
    at: TimePoint,
) -> obrain_core::Result<Option<EntityId>> {
    let buscado = nombre
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    for (c, _) in vivas(store, at)? {
        if self::nombre(store, &c, at)?.to_lowercase() == buscado {
            return Ok(Some(c));
        }
    }
    Ok(None)
}

/// El documento de tu bóveda con esta ruta, sin distinguir mayúsculas: lo que
/// usa la terminal para `--meter` y `--sacar`.
pub fn documento_en(
    store: &Store,
    ruta: &str,
    at: TimePoint,
) -> obrain_core::Result<Option<EntityId>> {
    if let Some(e) = store.find_subject_by_value("path_observed_at", ruta, at)? {
        return Ok(Some(e));
    }
    let buscada = ruta.to_lowercase().replace('/', "\\");
    for a in store.query(None, Some("path_observed_at"), at)? {
        if a.object
            .as_text()
            .is_some_and(|t| t.to_lowercase().replace('/', "\\") == buscada)
        {
            return Ok(Some(a.subject));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nombre_se_limpia_y_no_puede_quedar_vacio() {
        assert_eq!(
            limpio("  Trámite   de la visa ").unwrap(),
            "Trámite de la visa"
        );
        assert!(matches!(limpio("   "), Err(Error::SinNombre)));
        assert!(matches!(
            limpio(&"x".repeat(NOMBRE_MAX + 1)),
            Err(Error::NombreLargo)
        ));
        assert!(
            limpio(&"ñ".repeat(NOMBRE_MAX)).is_ok(),
            "se cuentan letras, no bytes"
        );
    }
}
