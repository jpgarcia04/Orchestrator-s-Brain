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
use std::collections::HashMap;

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
    if let Some(e) = store.find_subject_by_value("folder_path", path, tp)? {
        cache.insert(path.to_string(), e.clone());
        return Ok(e);
    }
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

    // Encadenar carpeta → carpeta padre, para que la travesía pueda subir.
    folders.sort();
    folders.dedup();
    for dir in folders {
        let Some(i) = dir.rfind(['\\', '/']) else { continue };
        let parent = &dir[..i];
        // Cortar en la raíz del volumen: `C:` no es una carpeta útil.
        if parent.is_empty() || parent.ends_with(':') {
            continue;
        }
        let child = folder_entity(store, &mut cache, &dir, at, report)?;
        let up = folder_entity(store, &mut cache, parent, at, report)?;
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
