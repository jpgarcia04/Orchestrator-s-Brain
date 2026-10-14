//! Búsqueda léxica de extremo a extremo. El criterio de "hecho" de M2.
//!
//! Es la primera vez que el sistema responde algo que el usuario no había
//! escrito en ningún sitio: el nombre del archivo no contiene la palabra
//! buscada, su contenido sí. Todavía no es la pregunta de la visa —eso es M7—
//! pero es el primer eslabón de la cadena de §6 del maestro: BM25 es una de
//! las cuatro ramas de recuperación paralela, y aquí ya funciona.

use obrain_core::rusqlite::{ToSql, params_from_iter};
use obrain_core::{EntityId, Result, Store, TimePoint};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Hit {
    pub entity: EntityId,
    pub path: String,
    pub ordinal: i64,
    /// BM25 de FTS5: **más negativo es mejor**. Se muestra tal cual en vez de
    /// normalizarlo a un 0–100 inventado, porque una puntuación maquillada
    /// invita a compararla con la del índice vectorial, y no son comparables
    /// (por eso §6 fusiona con RRF y no con pesos).
    pub score: f64,
    pub snippet: String,
}

/// Rebanada literal de la proyección publicada, situada en el texto normalizado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    pub span: (usize, usize),
    pub text: String,
}

/// Una cita de pregunta necesita más contexto que la línea breve del resultado.
const PASSAGE_BUDGET: usize = 420;

/// Hasta `limit` pasajes distintos de una entidad. Se buscan primero las
/// coincidencias léxicas; el fragmento vectorial es una opción adicional.
/// Esta función sólo lee el índice: el llamante comprueba publicación y PIN.
pub fn passages_for(
    store: &Store,
    entity: &EntityId,
    raw_query: &str,
    semantic_chunk: Option<i64>,
    limit: usize,
) -> Result<Vec<Passage>> {
    if limit == 0 || store.content_pending(entity)? {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    if let Some(exact) = prepare(raw_query) {
        collect_passages(store, entity, &exact, raw_query, limit, &mut out)?;
    }
    if out.len() < limit {
        if let Some(relaxed) = partial(raw_query) {
            collect_passages(
                store,
                entity,
                &relaxed.match_query,
                &relaxed.evidence_terms,
                limit,
                &mut out,
            )?;
        }
    }
    if out.len() < limit {
        if let Some(id) = semantic_chunk {
            let row = store.conn().query_row(
                "SELECT byte_start, text FROM chunks WHERE id=?1 AND entity=?2",
                obrain_core::rusqlite::params![id, entity.as_str()],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            );
            if let Ok((base, text)) = row {
                add_passage(&mut out, base, &text, raw_query);
            }
        }
    }
    Ok(out)
}

fn collect_passages(
    store: &Store,
    entity: &EntityId,
    query: &str,
    evidence_query: &str,
    limit: usize,
    out: &mut Vec<Passage>,
) -> Result<()> {
    let mut stmt = store.conn().prepare(
        "SELECT c.byte_start, c.text FROM chunks_fts
         JOIN chunks c ON c.id=chunks_fts.rowid
         WHERE chunks_fts MATCH ?1 AND c.entity=?2
         ORDER BY bm25(chunks_fts) LIMIT 24",
    )?;
    let rows = stmt.query_map(
        obrain_core::rusqlite::params![query, entity.as_str()],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )?;
    for row in rows {
        let (base, text) = row?;
        add_passage(out, base, &text, evidence_query);
        if out.len() >= limit {
            break;
        }
    }
    Ok(())
}

fn add_passage(out: &mut Vec<Passage>, base: i64, text: &str, query: &str) {
    let Some(evidence) = crate::evidence::pick(text, query, PASSAGE_BUDGET) else {
        return;
    };
    let start = base.max(0) as usize + evidence.byte_start;
    let end = base.max(0) as usize + evidence.byte_end;
    if out.iter().any(|p| start < p.span.1 && end > p.span.0) {
        return;
    }
    out.push(Passage {
        span: (start, end),
        text: evidence.text,
    });
}

/// Traduce lo que escribe una persona a una expresión MATCH de FTS5.
///
/// Sin esto, buscar `consultaExpte_X0000000X-2025` es un error de sintaxis y
/// no una búsqueda. Cada término va entrecomillado —lo que lo convierte en
/// frase literal, con sus guiones y puntos tokenizados por FTS5— y un `*` al
/// final se conserva como búsqueda por prefijo.
pub fn prepare(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|t| {
            let prefix = t.ends_with('*');
            let core: String = t.trim_end_matches('*').replace('"', " ");
            (core.trim().to_string(), prefix)
        })
        .filter(|(c, _)| !c.is_empty())
        .map(|(c, prefix)| {
            if prefix {
                format!("\"{c}\"*")
            } else {
                format!("\"{c}\"")
            }
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

/// Una frase recordada puede contener palabras que no están en el archivo.
/// Si la conjunción literal no encuentra nada, exigimos dos términos distintos
/// de contenido en *el mismo fragmento*. Un OR simple daría resultados por una
/// sola palabra común y no justificaría por qué se recuperó el documento.
struct PartialQuery {
    match_query: String,
    evidence_terms: String,
}

fn partial(raw: &str) -> Option<PartialQuery> {
    if raw.split_whitespace().count() < 4 {
        return None;
    }
    let mut terms: Vec<(String, bool)> = Vec::new();
    for word in raw.split(|c: char| !c.is_alphanumeric()) {
        let mut word = word.to_lowercase();
        if word.len() < 3
            || matches!(
                word.as_str(),
                "del"
                    | "las"
                    | "los"
                    | "una"
                    | "unas"
                    | "unos"
                    | "con"
                    | "por"
                    | "para"
                    | "que"
                    | "donde"
                    | "cuando"
                    | "cuándo"
                    | "sobre"
                    | "como"
                    | "cómo"
                    | "era"
                    | "son"
                    | "sus"
                    | "este"
                    | "esta"
                    | "estos"
                    | "estas"
            )
        {
            continue;
        }
        let mut plural = false;
        if word.chars().count() >= 5 && word.ends_with("es") {
            let root = word.strip_suffix("es").unwrap();
            if root
                .chars()
                .last()
                .is_some_and(|c| !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'))
            {
                word = root.to_string();
                plural = true;
            }
        }
        if !plural && word.chars().count() >= 5 && word.ends_with('s') {
            word.pop();
            plural = true;
        }
        if terms.iter().any(|(t, _)| t == &word) {
            continue;
        }
        terms.push((word, plural));
        // La relajación es para frases breves, no para convertir párrafos en
        // centenares de combinaciones FTS5.
        if terms.len() > 12 {
            return None;
        }
    }
    if terms.len() < 2 {
        return None;
    }
    let fts_terms: Vec<String> = terms
        .iter()
        .map(|(term, plural)| {
            // La rama de rescate tolera flexión de número: «rutas» puede
            // recordar «ruta» y «red» puede aparecer como «redes». El prefijo
            // sólo se usa en palabras cortas o quitando una s final.
            if *plural || term.chars().count() <= 5 {
                format!("\"{term}\"*")
            } else {
                format!("\"{term}\"")
            }
        })
        .collect();
    let mut pairs = Vec::new();
    for i in 0..terms.len() {
        for j in i + 1..terms.len() {
            pairs.push(format!("({} AND {})", fts_terms[i], fts_terms[j]));
        }
    }
    Some(PartialQuery {
        match_query: pairs.join(" OR "),
        // La cita debe buscar las mismas raíces y prefijos que MATCH; con los
        // plurales originales puede elegir prosa ajena a la coincidencia.
        evidence_terms: terms
            .into_iter()
            .map(|(term, _)| term)
            .collect::<Vec<_>>()
            .join(" "),
    })
}

/// El recorte de FTS5 es buena prueba —rodea el término y lo marca— salvo
/// cuando cae al lado de un sello criptográfico, y entonces la evidencia que
/// exige I9 se lee como que el programa está roto.
///
/// Se conserva el recorte de FTS5 en el caso normal, con su resaltado, y sólo
/// se sustituye por [`crate::evidence::pick`] cuando trae texto que no escribió
/// una persona. El razonamiento y la medición están en ese módulo.
fn guard(fts: String, text: &str, raw_query: &str) -> String {
    if !fts
        .split_whitespace()
        .any(crate::evidence::is_machine_token)
    {
        return fts;
    }
    crate::evidence::pick(text, raw_query, crate::evidence::BUDGET)
        .map(|e| e.text)
        .unwrap_or_default()
}

/// Busca en el índice léxico y devuelve **un resultado por archivo**: el mejor
/// de sus fragmentos.
///
/// Sin la deduplicación, un documento largo que menciona el término diez veces
/// ocuparía los diez primeros puestos y taparía a los demás.
pub fn search(store: &Store, raw_query: &str, limit: usize, at: TimePoint) -> Result<Vec<Hit>> {
    search_within(store, raw_query, limit, at, None)
}

/// [`search`] **sólo entre estos documentos**: los de una colección
/// (ADR-0031).
///
/// El filtro va dentro de la consulta, antes del `LIMIT`, y no después: si
/// se buscara como siempre y luego se quitara lo de fuera, los documentos de
/// la colección que estaban en el puesto 60 no saldrían nunca y la lista
/// podría quedarse vacía teniendo respuesta. `None` es buscar en todo.
pub fn search_within(
    store: &Store,
    raw_query: &str,
    limit: usize,
    at: TimePoint,
    within: Option<&HashSet<EntityId>>,
) -> Result<Vec<Hit>> {
    let Some(query) = prepare(raw_query) else {
        return Ok(Vec::new());
    };
    let exact = search_match(store, &query, raw_query, limit, at, within, false)?;
    if !exact.is_empty() || limit == 0 {
        return Ok(exact);
    }
    match partial(raw_query) {
        Some(query) => search_match(
            store,
            &query.match_query,
            &query.evidence_terms,
            limit,
            at,
            within,
            true,
        ),
        None => Ok(exact),
    }
}

fn search_match(
    store: &Store,
    query: &str,
    evidence_query: &str,
    limit: usize,
    at: TimePoint,
    within: Option<&HashSet<EntityId>>,
    relaxed: bool,
) -> Result<Vec<Hit>> {
    let dentro: Vec<&str> = within
        .map(|w| w.iter().map(EntityId::as_str).collect())
        .unwrap_or_default();
    if within.is_some() && dentro.is_empty() {
        return Ok(Vec::new());
    }
    let filtro = if within.is_some() {
        let huecos: Vec<String> = (0..dentro.len()).map(|i| format!("?{}", i + 3)).collect();
        format!("AND c.entity IN ({})", huecos.join(", "))
    } else {
        String::new()
    };

    let mut stmt = store.conn().prepare(&format!(
        "SELECT c.entity, c.ordinal, bm25(chunks_fts) AS score,
                snippet(chunks_fts, 0, '«', '»', ' … ', 14), c.text
         FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1
           AND NOT EXISTS (SELECT 1 FROM content_publication p
                            WHERE p.entity=c.entity AND p.state='pending') {filtro}
         ORDER BY score
         LIMIT ?2"
    ))?;
    // Se piden de sobra porque varios fragmentos del mismo archivo colapsan
    // en un solo resultado.
    let tope = (limit * 10).max(50) as i64;
    let mut valores: Vec<&dyn ToSql> = vec![&query, &tope];
    valores.extend(dentro.iter().map(|e| e as &dyn ToSql));
    let rows = stmt.query_map(params_from_iter(valores), |r| {
        let fts: String = r.get(3)?;
        let text: String = r.get(4)?;
        Ok(Hit {
            entity: EntityId(r.get(0)?),
            ordinal: r.get(1)?,
            score: r.get(2)?,
            snippet: if relaxed {
                // La prueba de la coincidencia parcial es una rebanada literal
                // del fragmento publicado; no una explicación generada.
                crate::evidence::pick(&text, evidence_query, crate::evidence::BUDGET)
                    .map(|e| e.text)
                    .unwrap_or_default()
            } else {
                guard(fts, &text, evidence_query)
            },
            path: String::new(),
        })
    })?;

    let mut seen: Vec<Hit> = Vec::new();
    for hit in rows {
        let mut hit = hit?;
        if seen.iter().any(|h| h.entity == hit.entity) {
            continue;
        }
        // Sin ruta vigente, el documento ya no está en tu bóveda: está en la
        // papelera y no se busca (ADR-0025). Su texto sigue en el índice hasta
        // que se olvide, para poder verlo desde la papelera.
        let Some(path) = store
            .resolve(&hit.entity, "path_observed_at", at)?
            .and_then(|a| a.object.as_text().map(str::to_string))
        else {
            continue;
        };
        hit.path = path;
        seen.push(hit);
        if seen.len() >= limit {
            break;
        }
    }
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_consulta_normal_se_convierte_en_and_de_frases() {
        assert_eq!(
            prepare("matricula 2025").unwrap(),
            "\"matricula\" AND \"2025\""
        );
    }

    #[test]
    fn los_guiones_no_rompen_la_sintaxis_de_fts5() {
        // Sin entrecomillar, FTS5 interpreta `-` como operador y falla.
        assert_eq!(
            prepare("consultaExpte_Z27-2025").unwrap(),
            "\"consultaExpte_Z27-2025\""
        );
    }

    #[test]
    fn el_asterisco_final_sigue_siendo_prefijo() {
        assert_eq!(prepare("matric*").unwrap(), "\"matric\"*");
    }

    #[test]
    fn una_consulta_vacia_no_es_una_consulta() {
        assert!(prepare("   ").is_none());
        assert!(prepare("\"\"").is_none());
    }

    #[test]
    fn la_frase_parcial_exige_dos_terminos_y_admite_flexion() {
        let query = partial("Los apuntes de la práctica donde hablábamos de rutas de red").unwrap();
        assert!(query.match_query.contains("\"práctica\""));
        assert!(query.match_query.contains("\"ruta\"*"));
        assert!(query.match_query.contains("\"red\"*"));
        assert!(!query.match_query.contains("\"donde\""));
        assert!(query.evidence_terms.contains("ruta"));
        assert!(!query.evidence_terms.contains("rutas"));
        assert!(partial("nodo azafrán").is_none());
        assert!(partial("La ruta y rutas").is_none());
        assert!(partial("La red y redes").is_none());
    }
}
