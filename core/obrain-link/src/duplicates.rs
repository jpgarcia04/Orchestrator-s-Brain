//! `duplicate_of` — la arista más barata y más fiable que existe.
//!
//! Dos archivos con el mismo BLAKE3 son el mismo contenido. No hay heurística,
//! no hay confianza parcial, no hay modelo. Es el ejemplo puro de por qué la
//! columna determinista de §4.4 es donde está el valor inicial del grafo.

use crate::{Edge, FileView, LinkReport, write_edge};
use obrain_core::{Method, Result, Store, Value};
use std::collections::HashMap;

pub fn link(store: &Store, files: &[FileView], at: i64, report: &mut LinkReport) -> Result<()> {
    let mut by_hash: HashMap<&str, Vec<&FileView>> = HashMap::new();
    for f in files {
        // Un archivo vacío tiene el mismo hash que todos los demás archivos
        // vacíos. Es cierto y es inútil: enlazaría `.gitkeep`, `LOCK` y un
        // borrador en blanco como "el mismo contenido", creando una estrella
        // enorme entre cosas sin ninguna relación.
        //
        // Encontrado auditando el grafo real con `--sample`. Los contadores no
        // lo habrían delatado nunca.
        if f.size == 0 {
            continue;
        }
        if let Some(h) = &f.hash {
            by_hash.entry(h.as_str()).or_default().push(f);
        }
    }

    for (_, mut group) in by_hash {
        if group.len() < 2 {
            continue;
        }
        // Canónico = el más antiguo. Es el que probablemente sea el original,
        // y da un criterio estable: reejecutar elige siempre el mismo.
        group.sort_by_key(|f| (f.modified_ms, f.path.clone()));
        let canonical = group[0];

        // Estrella, no clique: N−1 aristas contra el canónico en vez de
        // N(N−1)/2 entre todos. Con un grupo de 40 copias eso es 39 aristas
        // en lugar de 780, y la información es exactamente la misma.
        for dup in &group[1..] {
            if write_edge(
                store,
                report,
                Edge {
                    subject: &dup.entity,
                    predicate: "duplicate_of",
                    object: Value::Entity(canonical.entity.clone()),
                    method: Method::Deterministic,
                    confidence: 1.0,
                    source: "linker:hash",
                },
                at,
            )? {
                report.duplicate_of += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use obrain_core::{Facet, NewAssertion, Store, TimePoint};

    fn file(store: &Store, path: &str, hash: &str, mtime: i64) -> FileView {
        let e = store.create_entity(1000).unwrap();
        store
            .assert(NewAssertion::new(
                e.clone(),
                "has_facet",
                Facet::Addressable.as_str(),
                Method::Deterministic,
                1000,
            ))
            .unwrap();
        FileView {
            entity: e,
            path: path.to_string(),
            hash: Some(hash.to_string()),
            modified_ms: mtime,
            size: 10,
            availability: "local".into(),
        }
    }

    #[test]
    fn el_mismo_contenido_produce_una_estrella_no_un_clique() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            file(&s, "a.pdf", "HASH1", 300),
            file(&s, "b.pdf", "HASH1", 100), // el mas antiguo -> canonico
            file(&s, "c.pdf", "HASH1", 200),
            file(&s, "solo.pdf", "HASH2", 100),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 2000, &mut r).unwrap();

        // 3 duplicados -> 2 aristas (N-1), no 3 (N(N-1)/2)
        assert_eq!(r.duplicate_of, 2);

        let tp = TimePoint::at(3000);
        let canonical = &files[1].entity;
        for f in [&files[0], &files[2]] {
            let a = s.resolve(&f.entity, "duplicate_of", tp).unwrap().unwrap();
            assert_eq!(
                a.object.as_entity(),
                Some(canonical),
                "todos apuntan al mas antiguo"
            );
            assert_eq!(a.method, Method::Deterministic);
            assert_eq!(a.confidence, 1.0);
        }
        // el archivo sin pareja no gana ninguna arista
        assert!(
            s.resolve(&files[3].entity, "duplicate_of", tp)
                .unwrap()
                .is_none()
        );
    }

    /// Regresión encontrada en el corpus real: todos los archivos de 0 bytes
    /// comparten hash y se enlazaban entre sí.
    #[test]
    fn los_archivos_vacios_no_son_duplicados_entre_si() {
        let s = Store::open_in_memory().unwrap();
        let vacio = blake3::Hasher::new().finalize().to_hex().to_string();
        let mut files = vec![
            file(&s, ".gitkeep", &vacio, 100),
            file(&s, "LOCK", &vacio, 200),
            file(&s, "borrador.md", &vacio, 300),
        ];
        for f in &mut files {
            f.size = 0;
        }
        let mut r = LinkReport::default();
        super::link(&s, &files, 2000, &mut r).unwrap();
        assert_eq!(
            r.duplicate_of, 0,
            "vacio == vacio es cierto pero no dice nada"
        );
    }

    #[test]
    fn reejecutar_no_duplica_aristas() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![file(&s, "a.pdf", "H", 100), file(&s, "b.pdf", "H", 200)];

        let mut r1 = LinkReport::default();
        super::link(&s, &files, 2000, &mut r1).unwrap();
        assert_eq!(r1.duplicate_of, 1);

        let mut r2 = LinkReport::default();
        super::link(&s, &files, 3000, &mut r2).unwrap();
        assert_eq!(r2.duplicate_of, 0, "idempotente");
        assert_eq!(r2.already_present, 1);
    }
}
