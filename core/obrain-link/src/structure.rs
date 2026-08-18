//! `contained_by` — la jerarquía de carpetas como grafo.
//!
//! Las carpetas dejan de ser una propiedad textual del archivo y pasan a ser
//! **entidades**. Eso convierte "la carpeta visa-2023" en algo que se puede
//! consultar, enlazar y, más adelante, promover a `Process`.
//!
//! Sólo se almacena `contained_by`. La inversa `contains` está declarada en el
//! vocabulario y se deriva al consultar: guardar ambas sería duplicar cada
//! arista del grafo (§3.4).

use crate::{write_edge, Edge, FileView, LinkReport};
use obrain_core::{EntityId, Facet, Method, NewAssertion, Result, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

/// ¿Es esta carpeta un árbol de trabajo de git?
///
/// Cuesta un acceso al sistema de archivos por carpeta **distinta** —219 en el
/// corpus real— y es la misma clase de señal que [`crate::provenance`] ya lee
/// del disco para `downloaded_from`: está en el sistema de archivos, es
/// determinista, y no la inventa nadie.
///
/// **Se mira `.git` y no un manifiesto** (`package.json`, `Cargo.toml`) por dos
/// medidas sobre el corpus real: los manifiestos anidan —hay ocho `Cargo.toml`
/// dentro de este mismo repositorio, y marcar cada uno partiría el proyecto en
/// ocho— y dos de los siete repositorios del disco del usuario no tienen
/// ninguno. `.git` está exactamente una vez, en la raíz, en los siete.
///
/// Se comprueba la existencia y no que sea un directorio: en un *worktree* o un
/// submódulo, `.git` es un archivo, y sigue significando lo mismo.
fn es_proyecto(path: &str) -> bool {
    std::path::Path::new(path).join(".git").exists()
}

/// Encuentra o crea la entidad de una carpeta, identificada por su ruta.
fn folder_entity(
    store: &Store,
    cache: &mut HashMap<String, EntityId>,
    path: &str,
    at: i64,
    report: &mut LinkReport,
) -> Result<EntityId> {
    if let Some(e) = cache.get(path) {
        return Ok(e.clone());
    }
    let tp = TimePoint::at(at);
    let e = match store.find_subject_by_value("folder_path", path, tp)? {
        Some(e) => e,
        None => {
            let e = store.create_entity(at)?;
            store.assert(
                NewAssertion::new(e.clone(), "folder_path", path, Method::Deterministic, at)
                    .source("linker:structure"),
            )?;
            store.assert(
                NewAssertion::new(
                    e.clone(),
                    "has_facet",
                    Facet::Container.as_str(),
                    Method::Deterministic,
                    at,
                )
                .source("linker:structure"),
            )?;
            report.hubs_created += 1;
            e
        }
    };

    // La faceta se comprueba también sobre carpetas que **ya existían**, y no
    // sólo sobre las recién creadas: se añadió después de que se escribieran, y
    // un índice no debería tener que borrarse para aprender algo nuevo. El log
    // no crece por repetirlo —una aserción idéntica se cuenta como «sin
    // cambio»— y `enlazar` es idempotente por diseño.
    if es_proyecto(path) {
        let ya = store
            .query(Some(&e), Some("has_facet"), tp)?
            .iter()
            .any(|a| a.object.as_text() == Some(Facet::Project.as_str()));
        if !ya {
            store.assert(
                NewAssertion::new(
                    e.clone(),
                    "has_facet",
                    Facet::Project.as_str(),
                    Method::Deterministic,
                    at,
                )
                .source("linker:structure"),
            )?;
        }
    }

    cache.insert(path.to_string(), e.clone());
    Ok(e)
}

pub fn link(store: &Store, files: &[FileView], at: i64, report: &mut LinkReport) -> Result<()> {
    let mut cache: HashMap<String, EntityId> = HashMap::new();
    let mut folders: Vec<String> = Vec::new();

    for f in files {
        let parent = f.parent();
        if parent.is_empty() {
            continue;
        }
        let folder = folder_entity(store, &mut cache, parent, at, report)?;
        if write_edge(
            store,
            report,
            Edge {
                subject: &f.entity,
                predicate: "contained_by",
                object: Value::Entity(folder),
                method: Method::Deterministic,
                confidence: 1.0,
                source: "linker:structure",
            },
            at,
        )? {
            report.contained_by += 1;
        }
        folders.push(parent.to_string());
    }

    // Encadenar carpeta → carpeta padre **hasta la raíz del volumen**.
    //
    // Subir un solo nivel no basta, y el fallo no se ve en los contadores: sólo
    // las carpetas que contienen archivos directamente entran en `folders`, así
    // que una carpeta intermedia que sólo tiene subcarpetas —`prototypes`,
    // `src`, un año dentro de un álbum de fotos— nunca recibía su arista hacia
    // arriba y el árbol quedaba partido en trozos sueltos. Se vio al agregar el
    // grafo por regiones: aparecían como raíces a profundidad 0.
    folders.sort();
    folders.dedup();
    let mut chained: HashSet<String> = HashSet::new();
    for dir in folders {
        let mut cur = dir;
        // `chained` corta en cuanto se llega a una rama ya recorrida: sin él,
        // mil carpetas hermanas recorrerían mil veces el mismo camino a la raíz.
        while chained.insert(cur.clone()) {
            let Some(i) = cur.rfind(['\\', '/']) else { break };
            let parent = cur[..i].to_string();
            // Cortar en la raíz del volumen: `C:` no es una carpeta útil.
            if parent.is_empty() || parent.ends_with(':') {
                break;
            }
            let child = folder_entity(store, &mut cache, &cur, at, report)?;
            let up = folder_entity(store, &mut cache, &parent, at, report)?;
            if write_edge(
                store,
                report,
                Edge {
                    subject: &child,
                    predicate: "contained_by",
                    object: Value::Entity(up),
                    method: Method::Deterministic,
                    confidence: 1.0,
                    source: "linker:structure",
                },
                at,
            )? {
                report.contained_by += 1;
            }
            cur = parent;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fv(store: &Store, path: &str) -> FileView {
        FileView {
            entity: store.create_entity(1000).unwrap(),
            path: path.to_string(),
            hash: None,
            modified_ms: 1000,
            size: 1,
            availability: "local".into(),
        }
    }

    #[test]
    fn las_carpetas_se_convierten_en_nodos_encadenados() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            fv(&s, r"C:\docs\visa\nomina.pdf"),
            fv(&s, r"C:\docs\visa\pasaporte.pdf"),
            fv(&s, r"C:\docs\otros\recibo.pdf"),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 2000, &mut r).unwrap();

        // hubs: \docs\visa, \docs\otros, \docs
        assert_eq!(r.hubs_created, 3);
        // 3 archivos + 2 carpetas que encadenan a \docs
        assert_eq!(r.contained_by, 5);

        let tp = TimePoint::at(3000);
        let a = s.resolve(&files[0].entity, "contained_by", tp).unwrap().unwrap();
        let b = s.resolve(&files[1].entity, "contained_by", tp).unwrap().unwrap();
        assert_eq!(a.object, b.object, "los dos de visa comparten carpeta");
        let visa = a.object.as_entity().unwrap();
        assert!(
            s.resolve(visa, "contained_by", tp).unwrap().is_some(),
            "y esa carpeta cuelga de docs"
        );
    }

    /// **Regresión.** Una carpeta que sólo contiene subcarpetas no aparecía en
    /// la lista de padres directos, así que nunca recibía su arista hacia
    /// arriba y el árbol se partía. No lo veía ningún contador: las aristas
    /// seguían saliendo, sólo que el grafo eran varios trozos sueltos.
    #[test]
    fn una_carpeta_intermedia_sin_archivos_propios_tambien_encadena() {
        let s = Store::open_in_memory().unwrap();
        // Nadie guarda nada directamente en `repo\fotos` ni en `fotos\2019`.
        let files = vec![fv(&s, r"C:\repo\fotos\2019\viaje\img.jpg")];
        let mut r = LinkReport::default();
        super::link(&s, &files, 2000, &mut r).unwrap();

        let tp = TimePoint::at(3000);
        let sube = |e: &EntityId| {
            s.resolve(e, "contained_by", tp)
                .unwrap()
                .and_then(|a| a.object.as_entity().cloned())
        };
        let viaje = sube(&files[0].entity).expect("el archivo cuelga de viaje");
        let y2019 = sube(&viaje).expect("viaje cuelga de 2019");
        let fotos = sube(&y2019).expect("2019 cuelga de fotos");
        let repo = sube(&fotos).expect("fotos cuelga de repo");
        assert!(sube(&repo).is_none(), "y repo ya es la raiz: no se sube a C:");
    }

    #[test]
    fn no_se_encadena_por_encima_de_la_raiz_del_volumen() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![fv(&s, r"C:\suelto.txt")];
        let mut r = LinkReport::default();
        super::link(&s, &files, 2000, &mut r).unwrap();
        assert_eq!(r.hubs_created, 1, "solo C:, y no intenta subir mas");
        assert_eq!(r.contained_by, 1);
    }
}
