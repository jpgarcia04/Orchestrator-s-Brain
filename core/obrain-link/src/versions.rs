//! `version_of` — cadenas de versiones a partir del nombre del archivo.
//!
//! `informe.pdf`, `informe_v2.pdf`, `informe (1).pdf`, `informe - copia.pdf`,
//! `informe_final.pdf`. Todo el mundo tiene esto y nadie lo ha ordenado nunca.
//!
//! **Es una heurística, no un hecho.** Se escribe con `Method::Inferred` y
//! confianza parcial: el usuario puede corregirla y su corrección gana para
//! siempre (invariante I5).
//!
//! ## La trampa que casi se cuela
//!
//! La primera versión de este enlazador normalizaba *cualquier* número final,
//! así que `IMG_1234.png` colapsaba a `IMG` — y entonces **todas las fotos de
//! una carpeta se encadenaban como versiones unas de otras**. Sobre un corpus
//! real produjo 369 aristas, la mayoría basura.
//!
//! La regla que lo corrige: **un grupo sólo es una cadena de versiones si al
//! menos uno de sus miembros lleva un marcador inequívoco.** `IMG_0001` y
//! `IMG_0002` no lo llevan; `informe` e `informe_v2` sí. Un número suelto de
//! tres o más dígitos es numeración de cámara o de escáner, no una versión.

use crate::{Edge, FileView, LinkReport, write_edge};
use obrain_core::{Method, Result, Store, Value};
use std::collections::HashMap;

/// Marcadores textuales inequívocos, en español e inglés.
const EXPLICIT_SUFFIXES: &[&str] = &[
    " - copia",
    " - copy",
    "-copia",
    "_copia",
    "-copy",
    "_copy",
    " copia",
    " copy",
    "_final",
    "-final",
    " final",
    "_definitivo",
    "-definitivo",
    "_old",
    "-old",
    ".old",
    "_backup",
    "-backup",
    "_bak",
    "-bak",
    "_nuevo",
    "_new",
    "-new",
];

/// Qué clase de marcador llevaba el nombre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// Ninguno: es el nombre "base". `informe.pdf`
    None,
    /// Sólo un número suelto. `Bloque 3`, `IMG_1234`, `data_1`.
    /// **Ambiguo**: puede ser una versión o un elemento de una serie.
    Numeric,
    /// Marcador inequívoco de versión. `_v2`, `(1)`, `- copia`, `_final`.
    Explicit,
}

#[derive(Debug, PartialEq)]
pub struct VersionMark {
    /// Nombre sin los marcadores.
    pub base: String,
    pub kind: MarkKind,
}

/// Quita los marcadores de versión del nombre. `None` si no había ninguno.
pub fn strip_version_markers(stem: &str) -> Option<VersionMark> {
    let mut s = stem.trim().to_string();
    let mut found = false;
    let mut kind = MarkKind::None;

    loop {
        let before = s.clone();
        let lower = s.to_lowercase();

        for suffix in EXPLICIT_SUFFIXES {
            if lower.ends_with(suffix) {
                s.truncate(s.len() - suffix.len());
                kind = MarkKind::Explicit;
                break;
            }
        }

        // "(1)", "(2)": marcador de copia de Windows. Inequívoco.
        let t = s.trim_end();
        if t.ends_with(')') {
            if let Some(open) = t.rfind('(') {
                let inner = &t[open + 1..t.len() - 1];
                if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                    s.truncate(open);
                    kind = MarkKind::Explicit;
                }
            }
        }

        // "_v2", "-v3", " v10": inequívoco.
        // Un número suelto queda como `Numeric`: ambiguo por sí solo.
        let t = s.trim_end();
        if let Some(pos) = t.rfind(['_', '-', ' ']) {
            let tail = &t[pos + 1..];
            let is_v = tail.len() >= 2
                && matches!(tail.as_bytes()[0], b'v' | b'V')
                && tail[1..].chars().all(|c| c.is_ascii_digit());
            let is_num = !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit());
            if is_v {
                s.truncate(pos);
                kind = MarkKind::Explicit;
            } else if is_num {
                s.truncate(pos);
                if kind == MarkKind::None {
                    kind = MarkKind::Numeric;
                }
            }
        }

        s = s.trim_end_matches([' ', '_', '-']).to_string();
        if s == before {
            break;
        }
        found = true;
        if s.is_empty() {
            return None;
        }
    }

    if found {
        Some(VersionMark { base: s, kind })
    } else {
        None
    }
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

pub fn link(store: &Store, files: &[FileView], at: i64, report: &mut LinkReport) -> Result<()> {
    // Agrupar por (carpeta, nombre normalizado, extensión). Cruzar carpetas
    // produciría muchísimos falsos positivos.
    struct Member<'a> {
        file: &'a FileView,
        kind: MarkKind,
    }
    let mut groups: HashMap<(&str, String, &str), Vec<Member<'_>>> = HashMap::new();

    for f in files {
        let (stem, ext) = split_name(f.file_name());
        let (base, kind) = match strip_version_markers(stem) {
            Some(m) => (m.base, m.kind),
            None => (stem.to_string(), MarkKind::None),
        };
        groups
            .entry((f.parent(), base, ext))
            .or_default()
            .push(Member { file: f, kind });
    }

    for (_, mut group) in groups {
        if group.len() < 2 {
            continue;
        }
        // Un grupo es una cadena de versiones si:
        //   a) alguien lleva marcador inequívoco (`_v2`, `- copia`, `_final`), o
        //   b) hay un nombre **sin número** más otros numerados: el original y
        //      sus revisiones (`presupuesto` + `presupuesto_2`).
        //
        // Si TODOS llevan número y no hay original, es una **serie**, no una
        // cadena: `Bloque 3.mp4` / `Bloque 5.mp4` son dos vídeos distintos,
        // igual que `IMG_0001` / `IMG_0002`. Ese caso se descarta.
        let any_explicit = group.iter().any(|m| m.kind == MarkKind::Explicit);
        let has_original = group.iter().any(|m| m.kind == MarkKind::None);
        let any_numeric = group.iter().any(|m| m.kind == MarkKind::Numeric);
        if !(any_explicit || (has_original && any_numeric)) {
            continue;
        }
        // Canónico = el más reciente: es la versión "actual".
        group.sort_by_key(|m| (std::cmp::Reverse(m.file.modified_ms), m.file.path.clone()));
        let canonical = group[0].file.entity.clone();

        for older in &group[1..] {
            if write_edge(
                store,
                report,
                Edge {
                    subject: &older.file.entity,
                    predicate: "version_of",
                    object: Value::Entity(canonical.clone()),
                    method: Method::Inferred,
                    confidence: 0.75,
                    source: "heuristic:filename-version",
                },
                at,
            )? {
                report.version_of += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use obrain_core::TimePoint;

    fn base_of(s: &str) -> Option<String> {
        strip_version_markers(s).map(|m| m.base)
    }

    #[test]
    fn reconoce_los_marcadores_inequivocos() {
        for (entrada, esperado) in [
            ("informe_v2", "informe"),
            ("informe-v10", "informe"),
            ("informe (1)", "informe"),
            ("informe - copia", "informe"),
            ("informe_final", "informe"),
            ("informe_final_definitivo", "informe"),
            ("nomina_old", "nomina"),
        ] {
            assert_eq!(
                base_of(entrada).as_deref(),
                Some(esperado),
                "fallo con {entrada}"
            );
            assert_eq!(
                strip_version_markers(entrada).unwrap().kind,
                MarkKind::Explicit,
                "{entrada} deberia ser inequivoco"
            );
        }
    }

    #[test]
    fn un_numero_suelto_es_ambiguo_por_si_solo() {
        for entrada in [
            "IMG_1234",
            "DSC_0042",
            "Bloque 3",
            "presupuesto_2",
            "data_1",
        ] {
            assert_eq!(
                strip_version_markers(entrada).unwrap().kind,
                MarkKind::Numeric,
                "{entrada}: un numero no dice si es version o serie"
            );
        }
    }

    #[test]
    fn un_nombre_limpio_no_produce_cadena() {
        assert_eq!(strip_version_markers("informe"), None);
        assert_eq!(strip_version_markers("nomina marzo"), None);
    }

    fn fv(store: &Store, path: &str, mtime: i64) -> FileView {
        FileView {
            entity: store.create_entity(1000).unwrap(),
            path: path.to_string(),
            hash: None,
            modified_ms: mtime,
            size: 1,
            availability: "local".into(),
        }
    }

    #[test]
    fn encadena_versiones_contra_la_mas_reciente() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            fv(&s, r"C:\d\informe.pdf", 100),
            fv(&s, r"C:\d\informe_v2.pdf", 200),
            fv(&s, r"C:\d\informe_final.pdf", 300),
            fv(&s, r"C:\d\otra_cosa.pdf", 400),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 5000, &mut r).unwrap();

        assert_eq!(r.version_of, 2, "dos antiguos apuntan al mas reciente");
        let tp = TimePoint::at(6000);
        let a = s
            .resolve(&files[0].entity, "version_of", tp)
            .unwrap()
            .unwrap();
        assert_eq!(a.object.as_entity(), Some(&files[2].entity));
        assert_eq!(a.method, Method::Inferred);
        assert!(
            s.resolve(&files[3].entity, "version_of", tp)
                .unwrap()
                .is_none()
        );
    }

    /// Primera regresión encontrada en corpus real: 246 fotos encadenadas.
    #[test]
    fn una_carpeta_de_fotos_no_se_encadena_como_versiones() {
        let s = Store::open_in_memory().unwrap();
        let files: Vec<FileView> = (1..=40)
            .map(|i| fv(&s, &format!(r"C:\fotos\IMG_{i:04}.png"), 1000 + i))
            .collect();
        let mut r = LinkReport::default();
        super::link(&s, &files, 5000, &mut r).unwrap();
        assert_eq!(
            r.version_of, 0,
            "40 fotos distintas, no una cadena de 39 versiones"
        );
    }

    /// Segunda regresión, encontrada al auditar con `--sample`:
    /// `Bloque 3.mp4 -> Bloque 5.mp4`. Son clases de un curso, no versiones.
    #[test]
    fn una_serie_numerada_no_es_una_cadena_de_versiones() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            fv(&s, r"C:\curso\Bloque 1.mp4", 100),
            fv(&s, r"C:\curso\Bloque 3.mp4", 200),
            fv(&s, r"C:\curso\Bloque 5.mp4", 300),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 5000, &mut r).unwrap();
        assert_eq!(
            r.version_of, 0,
            "todos numerados y sin original: es una serie"
        );
    }

    /// Pero si existe el original sin numerar, sí es una cadena.
    #[test]
    fn original_mas_numerados_si_es_una_cadena() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            fv(&s, r"C:\d\presupuesto.xlsx", 100),
            fv(&s, r"C:\d\presupuesto_2.xlsx", 200),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 5000, &mut r).unwrap();
        assert_eq!(r.version_of, 1);
    }

    #[test]
    fn pero_una_foto_con_copia_explicita_si_encadena() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            fv(&s, r"C:\fotos\IMG_1234.png", 100),
            fv(&s, r"C:\fotos\IMG_1234 - copia.png", 200),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 5000, &mut r).unwrap();
        assert_eq!(r.version_of, 1);
    }

    #[test]
    fn no_cruza_carpetas() {
        let s = Store::open_in_memory().unwrap();
        let files = vec![
            fv(&s, r"C:\a\informe_v1.pdf", 100),
            fv(&s, r"C:\b\informe_v2.pdf", 200),
        ];
        let mut r = LinkReport::default();
        super::link(&s, &files, 5000, &mut r).unwrap();
        assert_eq!(
            r.version_of, 0,
            "mismo nombre en carpetas distintas no es una cadena"
        );
    }
}
