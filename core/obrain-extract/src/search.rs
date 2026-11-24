//! Búsqueda léxica de extremo a extremo. El criterio de "hecho" de M2.
//!
//! Es la primera vez que el sistema responde algo que el usuario no había
//! escrito en ningún sitio: el nombre del archivo no contiene la palabra
//! buscada, su contenido sí. Todavía no es la pregunta de la visa —eso es M7—
//! pero es el primer eslabón de la cadena de §6 del maestro: BM25 es una de
//! las cuatro ramas de recuperación paralela, y aquí ya funciona.

use obrain_core::rusqlite::ToSql;
use obrain_core::{Elegibles, EntityId, Result, Store, TimePoint, elegibilidad};
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
/// de sus fragmentos, entre lo que la ventana puede enseñar
/// ([`Elegibles::Ventana`]).
///
/// Sin la deduplicación, un documento largo que menciona el término diez veces
/// ocuparía los diez primeros puestos y taparía a los demás.
pub fn search(store: &Store, raw_query: &str, limit: usize, at: TimePoint) -> Result<Vec<Hit>> {
    search_within(store, raw_query, limit, at, None)
}

/// [`search`] **sólo entre estos documentos**: los de una colección
/// (ADR-0031). `None` es buscar en todo.
pub fn search_within(
    store: &Store,
    raw_query: &str,
    limit: usize,
    at: TimePoint,
    within: Option<&HashSet<EntityId>>,
) -> Result<Vec<Hit>> {
    search_eligible(store, raw_query, limit, at, within, Elegibles::Ventana)
}

/// La búsqueda léxica con **todos sus filtros dentro de la consulta**, antes
/// del `LIMIT`: la colección, lo que puede salir según quién mire
/// ([`Elegibles`]: publicación, papelera, identidad y, para la puerta,
/// sensibilidad) y un solo resultado por documento.
///
/// Antes se pedían de sobra fragmentos (`10 × limit`, al menos 50) y la
/// papelera y la deduplicación se aplicaban después, en Rust. Con un
/// documento largo que llenaba esos 50 fragmentos, o con unos cuantos de la
/// papelera arriba, la lista salía corta o vacía teniendo respuesta: lo que
/// estaba en el puesto 51 no se veía nunca (`PLAN-CONFIANZA.md` punto 3). Ahora
/// el `LIMIT` cuenta documentos elegibles, no fragmentos.
pub fn search_eligible(
    store: &Store,
    raw_query: &str,
    limit: usize,
    at: TimePoint,
    within: Option<&HashSet<EntityId>>,
    elegibles: Elegibles,
) -> Result<Vec<Hit>> {
    let Some(query) = prepare(raw_query) else {
        return Ok(Vec::new());
    };
    let exact = search_match(
        store, &query, raw_query, limit, at, within, elegibles, false,
    )?;
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
            elegibles,
            true,
        ),
        None => Ok(exact),
    }
}

#[allow(clippy::too_many_arguments)]
fn search_match(
    store: &Store,
    query: &str,
    evidence_query: &str,
    limit: usize,
    at: TimePoint,
    within: Option<&HashSet<EntityId>>,
    elegibles: Elegibles,
    relaxed: bool,
) -> Result<Vec<Hit>> {
    let dentro: Vec<&str> = within
        .map(|w| w.iter().map(EntityId::as_str).collect())
        .unwrap_or_default();
    if limit == 0 || (within.is_some() && dentro.is_empty()) {
        return Ok(Vec::new());
    }

    // Tres pasos en una consulta. `coincidencias`: los fragmentos que casan,
    // con su BM25 (la función auxiliar sólo existe junto al MATCH).
    // `mejores`: de ésos, sólo los de documentos elegibles, numerados dentro
    // de su documento. Y fuera, el mejor de cada documento, ordenado y
    // cortado. La ruta sale de la misma expresión que decidió que hay ruta.
    let sql = format!(
        "WITH coincidencias AS (
             SELECT c.id AS chunk, c.entity AS entity, c.ordinal AS ordinal,
                    bm25(chunks_fts) AS score
               FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
              WHERE chunks_fts MATCH :q {dentro}
         ),
         mejores AS (
             SELECT m.chunk, m.entity, m.ordinal, m.score,
                    ROW_NUMBER() OVER (PARTITION BY m.entity ORDER BY m.score, m.ordinal)
                        AS puesto
               FROM coincidencias m
              WHERE {elegibles}
         )
         SELECT b.chunk, b.entity, b.ordinal, b.score, {ruta}
           FROM mejores b
          WHERE b.puesto = 1
          ORDER BY b.score, b.entity
          LIMIT :limite",
        dentro = elegibilidad::dentro_sql("c.entity", within.map(|_| dentro.len())),
        elegibles = elegibles.sql("m.entity"),
        ruta = elegibilidad::ruta_vigente_sql("b.entity"),
    );
    let limite = limit as i64;
    let nombres: Vec<String> = (0..dentro.len()).map(elegibilidad::nombre_dentro).collect();
    let mut valores: Vec<(&str, &dyn ToSql)> = vec![
        (":q", &query),
        (":limite", &limite),
        (":valid", &at.valid),
        (":known", &at.known),
    ];
    valores.extend(
        nombres
            .iter()
            .map(String::as_str)
            .zip(dentro.iter().map(|e| e as &dyn ToSql)),
    );

    let mut stmt = store.conn().prepare(&sql)?;
    let rows = stmt.query_map(valores.as_slice(), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            EntityId(r.get(1)?),
            r.get::<_, i64>(2)?,
            r.get::<_, f64>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mejores = rows.collect::<obrain_core::rusqlite::Result<Vec<_>>>()?;

    // El recorte de FTS5 sólo para los que salen: calcularlo para cada
    // fragmento que casa costaría tanto como la búsqueda entera.
    let mut recorte = store.conn().prepare(
        "SELECT snippet(chunks_fts, 0, '«', '»', ' … ', 14), c.text
           FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
          WHERE chunks_fts MATCH ?1 AND chunks_fts.rowid = ?2",
    )?;
    let mut out = Vec::with_capacity(mejores.len());
    for (chunk, entity, ordinal, score, path) in mejores {
        let (fts, text): (String, String) = recorte
            .query_row(obrain_core::rusqlite::params![query, chunk], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        let snippet = if relaxed {
            // La prueba de la coincidencia parcial es una rebanada literal
            // del fragmento publicado; no una explicación generada.
            crate::evidence::pick(&text, evidence_query, crate::evidence::BUDGET)
                .map(|e| e.text)
                .unwrap_or_default()
        } else {
            guard(fts, &text, evidence_query)
        };
        out.push(Hit {
            entity,
            path,
            ordinal,
            score,
            snippet,
        });
    }
    Ok(out)
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

    // ── La elegibilidad va antes del LIMIT (PLAN-CONFIANZA punto 3) ──
    //
    // Bibliotecas sintéticas en memoria: entidades, fragmentos y rutas
    // inventadas. Nunca la bóveda real.

    use obrain_core::{Method, NewAssertion, Value};

    const AHORA: TimePoint = TimePoint {
        valid: 1_000,
        known: 1_000,
    };

    fn base() -> Store {
        let store = Store::open_in_memory().unwrap();
        crate::index::migrate(store.conn()).unwrap();
        store
    }

    fn afirmar(store: &Store, e: &EntityId, predicado: &str, valor: &str, t: i64) {
        store
            .assert(NewAssertion {
                subject: e.clone(),
                predicate: predicado.into(),
                object: Value::Text(valor.into()),
                valid_from: Some(t),
                valid_to: None,
                method: Method::Deterministic,
                source: "prueba".into(),
                confidence: 1.0,
                evidence: None,
                asserted_at: t,
            })
            .unwrap();
    }

    /// Un documento vigente y publicado, con estos fragmentos.
    fn documento(store: &Store, t: i64, textos: &[&str]) -> EntityId {
        let e = store.create_entity(t).unwrap();
        let chunks: Vec<crate::chunk::Chunk> = textos
            .iter()
            .enumerate()
            .map(|(i, texto)| crate::chunk::Chunk {
                ordinal: i,
                byte_start: 0,
                byte_end: texto.len(),
                text: (*texto).to_string(),
            })
            .collect();
        crate::index::replace_chunks(store.conn(), &e, "paragraph-pack", 1, &chunks).unwrap();
        afirmar(
            store,
            &e,
            "path_observed_at",
            &format!("C:/sintetica/{e}.txt"),
            t,
        );
        store.prepare_content(&e, None).unwrap();
        assert!(store.publish_content(&e, None).unwrap());
        e
    }

    fn a_la_papelera(store: &Store, e: &EntityId) {
        for a in store
            .query(Some(e), Some("path_observed_at"), AHORA)
            .unwrap()
        {
            store.retract(&a.id, 500).unwrap();
        }
    }

    fn entidades(hits: &[Hit]) -> Vec<&EntityId> {
        hits.iter().map(|h| &h.entity).collect()
    }

    /// Un documento con 60 fragmentos que casan mejor que el siguiente ocupa
    /// un puesto. Antes se pedían 50 fragmentos y se deduplicaba después: el
    /// segundo documento no salía y la lista tenía uno solo.
    #[test]
    fn un_documento_largo_no_tapa_al_siguiente() {
        let store = base();
        let textos: Vec<String> = (0..60)
            .map(|i| format!("azafrán azafrán azafrán fragmento {i}"))
            .collect();
        let largo = documento(
            &store,
            1,
            &textos.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let otro = documento(
            &store,
            2,
            &["un texto mucho más largo que sólo nombra el azafrán una vez entre otras palabras"],
        );
        let hits = search(&store, "azafrán", 5, AHORA).unwrap();
        assert_eq!(entidades(&hits), vec![&largo, &otro]);
        assert!(hits.iter().all(|h| h.path.starts_with("C:/sintetica/")));
    }

    /// Lo de la papelera, lo pendiente de publicar y lo de identidad pendiente
    /// no salen ni ocupan sitio: con un límite de uno, sale el vigente.
    #[test]
    fn papelera_y_pendientes_no_ocupan_puesto() {
        let store = base();
        let mut fuera = Vec::new();
        for t in 1..=60 {
            let e = documento(&store, t, &["azafrán azafrán azafrán"]);
            match t % 3 {
                0 => a_la_papelera(&store, &e),
                1 => store.prepare_content(&e, None).unwrap(),
                _ => afirmar(
                    &store,
                    &e,
                    "identity_pending_path",
                    "C:/sintetica/x.txt",
                    500,
                ),
            }
            fuera.push(e);
        }
        let vigente = documento(
            &store,
            100,
            &["en este documento el azafrán aparece una sola vez"],
        );
        let hits = search(&store, "azafrán", 1, AHORA).unwrap();
        assert_eq!(entidades(&hits), vec![&vigente]);
    }

    /// Dentro de una colección con lo de dentro en la papelera, la colección
    /// devuelve su documento vigente aunque estuviera muy abajo en todo.
    #[test]
    fn la_coleccion_y_la_papelera_filtran_juntas() {
        let store = base();
        for t in 1..=60 {
            documento(&store, t, &["azafrán azafrán azafrán"]);
        }
        let tirado = documento(&store, 70, &["azafrán azafrán"]);
        a_la_papelera(&store, &tirado);
        let dentro = documento(
            &store,
            71,
            &["un texto largo donde el azafrán sale una vez y ya"],
        );
        let coleccion: HashSet<EntityId> = [tirado.clone(), dentro.clone()].into();
        let hits = search_within(&store, "azafrán", 1, AHORA, Some(&coleccion)).unwrap();
        assert_eq!(entidades(&hits), vec![&dentro]);
        assert!(
            search_within(&store, "azafrán", 5, AHORA, Some(&HashSet::new()))
                .unwrap()
                .is_empty()
        );
    }

    /// Para la puerta (ADR-0038): si los diez primeros son sensibles, sale el
    /// normal que viene detrás. Lo que nunca tuvo publicación no cuenta como
    /// clasificado. La ventana sigue viéndolos a todos.
    #[test]
    fn solo_normal_devuelve_el_normal_que_venia_detras() {
        let store = base();
        for t in 1..=10 {
            let e = documento(&store, t, &["azafrán azafrán azafrán"]);
            store
                .conn()
                .execute(
                    "UPDATE entities SET sensitivity = 'sensitive' WHERE id = ?1",
                    [e.as_str()],
                )
                .unwrap();
        }
        let sin_fila = documento(&store, 20, &["azafrán azafrán"]);
        store
            .conn()
            .execute(
                "DELETE FROM content_publication WHERE entity = ?1",
                [sin_fila.as_str()],
            )
            .unwrap();
        let normal = documento(
            &store,
            21,
            &["un texto largo donde el azafrán sale una vez y ya"],
        );

        let puerta =
            search_eligible(&store, "azafrán", 10, AHORA, None, Elegibles::SoloNormal).unwrap();
        assert_eq!(entidades(&puerta), vec![&normal]);

        let ventana = search(&store, "azafrán", 20, AHORA).unwrap();
        assert_eq!(ventana.len(), 12);
        assert_eq!(ventana.last().map(|h| &h.entity), Some(&normal));
    }

    /// La búsqueda de rescate (frase parcial) aplica los mismos filtros.
    #[test]
    fn la_frase_parcial_tambien_filtra_antes_de_cortar() {
        let store = base();
        for t in 1..=60 {
            let e = documento(&store, t, &["apuntes de rutas de red de la práctica"]);
            a_la_papelera(&store, &e);
        }
        let vigente = documento(&store, 100, &["apuntes de la práctica sobre la red"]);
        let hits = search(
            &store,
            "Los apuntes de la práctica donde hablábamos de rutas de red",
            1,
            AHORA,
        )
        .unwrap();
        assert_eq!(entidades(&hits), vec![&vigente]);
        assert!(!hits[0].snippet.is_empty());
    }
}
