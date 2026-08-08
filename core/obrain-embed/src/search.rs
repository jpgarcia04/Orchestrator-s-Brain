//! Recuperación híbrida: las dos ramas de `PROJECT-MASTER.md` §6 que existen
//! hoy, fusionadas con RRF.
//!
//! ```text
//! consulta
//!   ├── BM25          ← M2, obrain_extract::search
//!   └── kNN vectorial ← M4
//!         ↓
//!   fusion RRF (k = 60)
//!         ↓
//!   resultados con procedencia
//! ```
//!
//! Faltan las otras dos ramas de §6 —travesía de grafo desde entidades
//! resueltas y filtro temporal—, y faltan porque necesitan resolución de
//! entidad, que es M7. La forma del pipeline ya las admite: son dos `Ranking`
//! más en la llamada a [`crate::fuse::rrf`].

use crate::embedder::{Embedder, Role};
use crate::fuse::{rrf, Fused, Ranking};
use crate::{index, Result};
use obrain_core::{EntityId, Store, TimePoint};

/// Un resultado de la búsqueda híbrida, con de dónde vino.
#[derive(Debug, Clone)]
pub struct Hit {
    pub entity: EntityId,
    pub path: String,
    pub score: f64,
    /// `(rama, puesto)`. I9: ninguna respuesta sin procedencia.
    pub ranks: Vec<(String, usize)>,
    /// El mejor fragmento de este archivo según la rama vectorial, si la hubo.
    pub chunk: Option<i64>,
    pub snippet: String,
}

/// Cuántos candidatos pide cada rama antes de fusionar.
///
/// Más que el límite final a propósito: si una rama sólo aporta 10 candidatos,
/// la fusión no puede rescatar el documento que esa rama pone en el puesto 15
/// y la otra no ve. §6 habla de rerank sobre el top-50; éste es el mismo orden.
pub const CANDIDATES: usize = 50;

/// El texto de un fragmento, recortado para enseñarlo.
fn snippet_of(store: &Store, chunk: i64) -> String {
    let text: String = store
        .conn()
        .query_row("SELECT text FROM chunks WHERE id = ?1", [chunk], |r| r.get(0))
        .unwrap_or_default();
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(160).collect()
}

fn path_of(store: &Store, e: &EntityId, at: TimePoint) -> String {
    store
        .resolve(e, "path_observed_at", at)
        .ok()
        .flatten()
        .and_then(|a| a.object.as_text().map(str::to_string))
        .unwrap_or_else(|| format!("<{e}>"))
}

/// Sólo la rama vectorial, **deduplicada por archivo**.
///
/// La deduplicación no es cosmética: sin ella un documento largo cuyos veinte
/// fragmentos son todos parecidos a la consulta ocupa los veinte primeros
/// puestos, y RRF fusiona una lista que en realidad tiene un solo documento.
/// Es el mismo motivo por el que `obrain_extract::search` deduplica.
pub fn semantic(
    store: &Store,
    embedder: &dyn Embedder,
    query: &str,
    limit: usize,
    at: TimePoint,
) -> Result<Vec<Hit>> {
    let model = embedder.model();
    let qv = embedder.embed(&[query], Role::Query)?;
    let Some(qv) = qv.first() else {
        return Ok(Vec::new());
    };
    // Se piden de sobra por la misma razón que en BM25: varios fragmentos del
    // mismo archivo colapsan en un solo resultado.
    let neighbours = index::knn(store.conn(), qv, model, (limit * 10).max(200))?;

    let mut out: Vec<Hit> = Vec::new();
    for n in neighbours {
        if out.iter().any(|h| h.entity == n.entity) {
            continue;
        }
        out.push(Hit {
            path: path_of(store, &n.entity, at),
            snippet: snippet_of(store, n.chunk),
            entity: n.entity,
            score: n.score as f64,
            ranks: vec![("knn".to_string(), out.len() + 1)],
            chunk: Some(n.chunk),
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// Las dos ramas y su fusión. Devuelve `(fusionado, bm25, knn)` porque el
/// informe tiene que poder enseñar las tres listas lado a lado: decir que la
/// fusión mejora sin enseñar lo que fusiona no es una medición.
#[allow(clippy::type_complexity)]
pub fn hybrid(
    store: &Store,
    embedder: &dyn Embedder,
    query: &str,
    limit: usize,
    at: TimePoint,
) -> Result<(Vec<Hit>, Vec<obrain_extract::Hit>, Vec<Hit>)> {
    let lexical = obrain_extract::search(store, query, CANDIDATES, at)?;
    let vectorial = semantic(store, embedder, query, CANDIDATES, at)?;

    let fused: Vec<Fused<EntityId>> = rrf(
        &[
            Ranking { name: "bm25", items: lexical.iter().map(|h| h.entity.clone()).collect() },
            Ranking { name: "knn", items: vectorial.iter().map(|h| h.entity.clone()).collect() },
        ],
        limit,
    );

    let hits = fused
        .into_iter()
        .map(|f| {
            let v = vectorial.iter().find(|h| h.entity == f.item);
            let l = lexical.iter().find(|h| h.entity == f.item);
            Hit {
                path: v
                    .map(|h| h.path.clone())
                    .or_else(|| l.map(|h| h.path.clone()))
                    .unwrap_or_else(|| path_of(store, &f.item, at)),
                snippet: v
                    .map(|h| h.snippet.clone())
                    .or_else(|| l.map(|h| h.snippet.clone()))
                    .unwrap_or_default(),
                chunk: v.and_then(|h| h.chunk),
                entity: f.item,
                score: f.score,
                ranks: f.ranks,
            }
        })
        .collect();
    Ok((hits, lexical, vectorial))
}
