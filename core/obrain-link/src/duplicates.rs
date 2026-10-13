//! `duplicate_of` — la arista más barata y más fiable que existe.
//!
//! Dos archivos con el mismo BLAKE3 son el mismo contenido. No hay heurística,
//! no hay confianza parcial, no hay modelo. Es el ejemplo puro de por qué la
//! columna determinista de §4.4 es donde está el valor inicial del grafo.

use crate::{Edge, FileView, LinkReport, write_edge};
use obrain_core::{EntityId, Method, Result, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

const SOURCE: &str = "linker:hash";

pub fn link(store: &Store, files: &[FileView], at: i64, report: &mut LinkReport) -> Result<()> {
    let tp = TimePoint::at(at);
    // Una identidad sin resolver no prueba que la relación anterior haya
    // dejado de ser cierta. Se espera a una pasada concluyente.
    let pending: HashSet<EntityId> = store
        .query(None, Some("identity_pending_path"), tp)?
        .into_iter()
        .map(|a| a.subject)
        .collect();
    let mut by_hash: HashMap<&str, Vec<&FileView>> = HashMap::new();
    for f in files {
        // Un archivo vacío tiene el mismo hash que todos los demás archivos
        // vacíos. Es cierto y es inútil: enlazaría `.gitkeep`, `LOCK` y un
        // borrador en blanco como "el mismo contenido", creando una estrella
        // enorme entre cosas sin ninguna relación.
        //
        // Encontrado auditando el grafo real con `--sample`. Los contadores no
        // lo habrían delatado nunca.
        if f.size == 0 || pending.contains(&f.entity) {
            continue;
        }
        if let Some(h) = &f.hash {
            by_hash.entry(h.as_str()).or_default().push(f);
        }
    }

    let mut wanted: HashSet<(EntityId, EntityId)> = HashSet::new();
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
            wanted.insert((dup.entity.clone(), canonical.entity.clone()));
        }
    }

    // Sólo se retracta lo que afirmó este enlazador. El corte temporal deja
    // consultable la historia y conserva correcciones y otras procedencias.
    for a in store.query(None, Some("duplicate_of"), tp)? {
        if a.method != Method::Deterministic || a.source != SOURCE {
            continue;
        }
        let Some(object) = a.object.as_entity() else {
            continue;
        };
        if pending.contains(&a.subject) || pending.contains(object) {
            continue;
        }
        if !wanted.contains(&(a.subject.clone(), object.clone())) {
            store.retract(&a.id, at)?;
        }
    }

    let mut wanted: Vec<_> = wanted.into_iter().collect();
    wanted.sort();
    for (subject, object) in wanted {
        if write_edge(
            store,
            report,
            Edge {
                subject: &subject,
                predicate: "duplicate_of",
                object: Value::Entity(object),
                method: Method::Deterministic,
                confidence: 1.0,
                source: SOURCE,
            },
            at,
        )? {
            report.duplicate_of += 1;
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

    #[test]
    fn al_cambiar_el_canonico_reconcilia_la_estrella_y_respeta_al_usuario() {
        let s = Store::open_in_memory().unwrap();
        let mut files = vec![
            file(&s, "a.pdf", "H", 100),
            file(&s, "b.pdf", "H", 200),
            file(&s, "c.pdf", "H", 300),
        ];
        super::link(&s, &files, 2000, &mut LinkReport::default()).unwrap();
        let before = s
            .query(None, Some("duplicate_of"), TimePoint::at(2000))
            .unwrap();
        assert_eq!(before.len(), 2);

        s.assert(
            NewAssertion::new(
                files[1].entity.clone(),
                "duplicate_of",
                Value::Entity(files[0].entity.clone()),
                Method::User,
                2500,
            )
            .source("user:test"),
        )
        .unwrap();
        files[0].modified_ms = 400; // mismos bytes, nuevo canónico: b
        super::link(&s, &files, 3000, &mut LinkReport::default()).unwrap();

        let active = s
            .query(None, Some("duplicate_of"), TimePoint::at(3001))
            .unwrap();
        assert_eq!(active.len(), 3);
        assert!(
            active
                .iter()
                .any(|a| a.method == Method::User && a.subject == files[1].entity)
        );
        assert!(active.iter().any(|a| {
            a.source == "linker:hash"
                && a.subject == files[2].entity
                && a.object.as_entity() == Some(&files[1].entity)
        }));
        assert!(active.iter().any(|a| {
            a.source == "linker:hash"
                && a.subject == files[0].entity
                && a.object.as_entity() == Some(&files[1].entity)
        }));
        assert_eq!(
            s.query(None, Some("duplicate_of"), TimePoint::at(2500))
                .unwrap()
                .len(),
            3,
            "la historia conserva las dos aristas originales y la corrección"
        );
    }

    #[test]
    fn quitar_y_reponer_una_copia_retracta_y_reafirma_sin_duplicar() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![file(&s, "a.pdf", "H", 100), file(&s, "b.pdf", "H", 200)];
        super::link(&s, &files, 2000, &mut LinkReport::default()).unwrap();
        super::link(&s, &files[..1], 3000, &mut LinkReport::default()).unwrap();
        assert!(
            s.query(None, Some("duplicate_of"), TimePoint::at(3001))
                .unwrap()
                .is_empty()
        );

        super::link(&s, &files, 4000, &mut LinkReport::default()).unwrap();
        super::link(&s, &files, 5000, &mut LinkReport::default()).unwrap();
        let active = s
            .query(None, Some("duplicate_of"), TimePoint::at(5001))
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].asserted_at, 4000);
        assert_eq!(
            s.query(None, Some("duplicate_of"), TimePoint::at(2500))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn identidad_pendiente_espera_y_otras_procedencias_se_conservan() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![file(&s, "a.pdf", "H", 100), file(&s, "b.pdf", "H", 200)];
        super::link(&s, &files, 2000, &mut LinkReport::default()).unwrap();
        let pending = s
            .assert(
                NewAssertion::new(
                    files[1].entity.clone(),
                    "identity_pending_path",
                    "b.pdf",
                    Method::Deterministic,
                    2500,
                )
                .source("scan@m1"),
            )
            .unwrap();
        super::link(&s, &files[..1], 3000, &mut LinkReport::default()).unwrap();
        assert_eq!(
            s.query(None, Some("duplicate_of"), TimePoint::at(3001))
                .unwrap()
                .len(),
            1
        );

        s.retract(&pending, 4000).unwrap();
        s.assert(
            NewAssertion::new(
                files[1].entity.clone(),
                "duplicate_of",
                Value::Entity(files[0].entity.clone()),
                Method::Imported,
                4000,
            )
            .source("import:test"),
        )
        .unwrap();
        super::link(&s, &files[..1], 5000, &mut LinkReport::default()).unwrap();
        let active = s
            .query(None, Some("duplicate_of"), TimePoint::at(5001))
            .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].source, "import:test");
    }
}
