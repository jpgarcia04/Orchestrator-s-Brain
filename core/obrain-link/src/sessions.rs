//! Sesiones de trabajo — agrupar por proximidad temporal de `mtime`.
//!
//! Los archivos que tocaste en la misma tarde suelen pertenecer a lo mismo.
//! Es una señal sorprendentemente fuerte y absolutamente gratuita: la fecha de
//! modificación ya está en el log desde M1.
//!
//! Es el enlazador que más se acerca a "construir conocimiento" sin IA: crea
//! una entidad —la sesión— que **nadie escribió**, y que más adelante es
//! candidata natural a promoverse a `Process`.
//!
//! ## Por qué hay un techo de tamaño
//!
//! Una descarga masiva, un `git clone` o restaurar una copia de seguridad
//! escriben miles de `mtime` casi idénticos. Eso no es una sesión de trabajo:
//! es una operación de máquina. Agruparlos produciría un nodo gigante y falso
//! que contamina el grafo. Por eso los grupos demasiado grandes se descartan.

use crate::{write_edge, Edge, FileView, LinkReport};
use obrain_core::{Facet, Method, NewAssertion, Result, Store, Value};

/// Hueco máximo entre dos archivos consecutivos para seguir en la misma sesión.
pub const GAP_MS: i64 = 45 * 60 * 1000; // 45 minutos
/// Mínimo de archivos para que un grupo merezca ser una sesión.
pub const MIN_FILES: usize = 3;
/// Por encima de esto es una operación masiva, no una sesión humana.
pub const MAX_FILES: usize = 150;

pub fn link(store: &Store, files: &[FileView], at: i64, report: &mut LinkReport) -> Result<()> {
    let mut ordered: Vec<&FileView> = files.iter().filter(|f| f.modified_ms > 0).collect();
    ordered.sort_by_key(|f| f.modified_ms);

    let mut group: Vec<&FileView> = Vec::new();
    let flush = |group: &mut Vec<&FileView>, report: &mut LinkReport| -> Result<()> {
        if group.len() >= MIN_FILES && group.len() <= MAX_FILES {
            let from = group[0].modified_ms;
            let to = group[group.len() - 1].modified_ms;

            // ¿Ya existía esta sesión? Enlazar es idempotente en todas partes menos
            // aquí: `structure` busca la carpeta por su ruta antes de crearla, y
            // esto creaba un concentrador nuevo en cada pasada. Medido sobre el
            // corpus real, una segunda ejecución de `obrain-link` producía **80
            // sesiones duplicadas y 961 `occurred_during` de más** — cada archivo
            // colgando de dos sesiones idénticas.
            //
            // El intervalo identifica la sesión porque es lo que la define: los
            // mismos archivos con los mismos `mtime` la reconstruyen igual. Y se
            // busca por el eje de conocimiento y no por el de validez, porque
            // `session_span` vive en el de validez (I6) y un `at.valid` de ahora
            // no encontraría nunca una sesión del año pasado.
            let existente = store.find_subject_by_timestamp_whenever("session_span", from, at)?;
            let session = match existente {
                Some(e) => e,
                None => {
                    let session = store.create_entity(at)?;
                    for facet in [Facet::Temporal, Facet::Container] {
                        store.assert(
                            NewAssertion::new(
                                session.clone(),
                                "has_facet",
                                facet.as_str(),
                                Method::Deterministic,
                                at,
                            )
                            .source("linker:sessions"),
                        )?;
                    }
                    // El intervalo va en el eje de validez: la sesión *ocurrió*
                    // entonces, aunque el sistema se entere ahora (I6).
                    store.assert(
                        NewAssertion::new(
                            session.clone(),
                            "session_span",
                            Value::Timestamp(from),
                            Method::Deterministic,
                            at,
                        )
                        .valid(Some(from), Some(to.max(from + 1)))
                        .source("linker:sessions"),
                    )?;
                    report.hubs_created += 1;
                    session
                }
            };

            // Hub: N aristas contra la sesión, no N² entre archivos.
            for f in group.iter() {
                if write_edge(
                    store,
                    report,
                    Edge {
                        subject: &f.entity,
                        predicate: "occurred_during",
                        object: Value::Entity(session.clone()),
                        method: Method::Inferred,
                        confidence: 0.7,
                        source: "heuristic:mtime-session",
                    },
                    at,
                )? {
                    report.occurred_during += 1;
                }
            }
        }
        group.clear();
        Ok(())
    };

    for f in ordered {
        if let Some(last) = group.last() {
            if f.modified_ms - last.modified_ms > GAP_MS {
                flush(&mut group, report)?;
            }
        }
        group.push(f);
    }
    flush(&mut group, report)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use obrain_core::TimePoint;

    const MIN: i64 = 60 * 1000;

    fn fv(store: &Store, name: &str, mtime: i64) -> FileView {
        FileView {
            entity: store.create_entity(1000).unwrap(),
            path: format!(r"C:\d\{name}"),
            hash: None,
            modified_ms: mtime,
            size: 1,
            availability: "local".into(),
        }
    }

    /// **Enlazar dos veces no duplica nada.**
    ///
    /// `structure` busca la carpeta por su ruta antes de crearla; las sesiones
    /// no buscaban nada y creaban un concentrador nuevo en cada pasada. Sobre el
    /// corpus real eso eran **80 sesiones de más y 961 `occurred_during`
    /// duplicados** en la segunda ejecución — cada archivo colgando de dos
    /// sesiones idénticas— y no lo delataba ningún contador, porque el informe
    /// dice cuántas aristas escribió, no cuántas sobraban.
    ///
    /// Importa más de lo que parece: reenlazar es lo que hay que hacer después
    /// de cada escaneo, o sea siempre.
    #[test]
    fn enlazar_dos_veces_no_duplica_las_sesiones() {
        let s = Store::open_in_memory().unwrap();
        let base = 1_700_000_000_000i64;
        let files = vec![
            fv(&s, "a", base),
            fv(&s, "b", base + 5 * MIN),
            fv(&s, "c", base + 12 * MIN),
        ];

        let mut primera = LinkReport::default();
        super::link(&s, &files, 9_000, &mut primera).unwrap();
        assert_eq!(primera.hubs_created, 1);

        let mut segunda = LinkReport::default();
        super::link(&s, &files, 10_000, &mut segunda).unwrap();
        assert_eq!(segunda.hubs_created, 0, "la sesion ya estaba escrita");

        // Y cada archivo sigue colgando de **una** sesion, no de dos.
        let tp = TimePoint::at(11_000);
        for f in &files {
            let cuantas = s
                .query(Some(&f.entity), Some("occurred_during"), tp)
                .unwrap()
                .len();
            assert_eq!(cuantas, 1, "un archivo, una sesion");
        }
    }

    #[test]
    fn agrupa_lo_tocado_en_la_misma_tarde_y_separa_lo_de_otro_dia() {
        let s = Store::open_in_memory().unwrap();
        let base = 1_700_000_000_000i64;
        let files = vec![
            fv(&s, "a", base),
            fv(&s, "b", base + 5 * MIN),
            fv(&s, "c", base + 12 * MIN),
            // tres dias despues: otra sesion
            fv(&s, "d", base + 3 * 24 * 60 * MIN),
            fv(&s, "e", base + 3 * 24 * 60 * MIN + 2 * MIN),
            fv(&s, "f", base + 3 * 24 * 60 * MIN + 9 * MIN),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 9_000, &mut r).unwrap();

        assert_eq!(r.hubs_created, 2, "dos sesiones distintas");
        assert_eq!(r.occurred_during, 6);

        let tp = TimePoint::at(9_999);
        let s1 = s.resolve(&files[0].entity, "occurred_during", tp).unwrap().unwrap();
        let s2 = s.resolve(&files[2].entity, "occurred_during", tp).unwrap().unwrap();
        let s3 = s.resolve(&files[3].entity, "occurred_during", tp).unwrap().unwrap();
        assert_eq!(s1.object, s2.object);
        assert_ne!(s1.object, s3.object);
    }

    #[test]
    fn dos_archivos_sueltos_no_son_una_sesion() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![fv(&s, "a", 1000), fv(&s, "b", 2000)];
        let mut r = LinkReport::default();
        super::link(&s, &files, 9_000, &mut r).unwrap();
        assert_eq!(r.hubs_created, 0);
    }

    #[test]
    fn una_operacion_masiva_no_se_confunde_con_trabajo_humano() {
        let s = Store::open_in_memory().unwrap();
        // 400 archivos escritos casi en el mismo instante: un clone o una restauracion
        let files: Vec<FileView> = (0..400)
            .map(|i| fv(&s, &format!("f{i}"), 1_700_000_000_000 + i))
            .collect();
        let mut r = LinkReport::default();
        super::link(&s, &files, 9_000, &mut r).unwrap();
        assert_eq!(r.hubs_created, 0, "por encima de MAX_FILES se descarta");
        assert_eq!(r.occurred_during, 0);
    }

    #[test]
    fn la_sesion_es_inferida_no_determinista() {
        let s = Store::open_in_memory().unwrap();
        let base = 1_700_000_000_000i64;
        let files = vec![fv(&s, "a", base), fv(&s, "b", base + MIN), fv(&s, "c", base + 2 * MIN)];
        let mut r = LinkReport::default();
        super::link(&s, &files, 9_000, &mut r).unwrap();

        let a = s
            .resolve(&files[0].entity, "occurred_during", TimePoint::at(9_999))
            .unwrap()
            .unwrap();
        assert_eq!(a.method, Method::Inferred, "es una heuristica, puede estar mal");
        assert!(a.confidence < 1.0);
        assert_eq!(a.source, "heuristic:mtime-session");
    }
}
