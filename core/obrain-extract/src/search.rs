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
         WHERE chunks_fts MATCH ?1 {filtro}
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
            snippet: guard(fts, &text, raw_query),
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
}
