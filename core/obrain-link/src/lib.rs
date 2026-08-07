//! # obrain-link — M3 · Relacionar sin IA
//!
//! El hito más subestimado del roadmap. Convierte el log plano que produce M1
//! en un grafo con densidad real, y lo hace con **cero llamadas a un modelo**.
//!
//! Todas las señales que explota ya están en el log o son metadatos del sistema
//! de archivos (`PROJECT-MASTER.md` §4.4). Es el 80% del valor inicial del grafo
//! por una fracción del coste de cualquier cosa que involucre IA.
//!
//! ## La regla que gobierna este crate: **hubs, no cliques**
//!
//! Un grupo de N archivos relacionados **nunca** se conecta todos-con-todos.
//! Eso son N(N−1)/2 aristas: 200 archivos de una misma sesión producirían
//! 19.900 aristas que no dicen nada que no dijera una sola.
//!
//! En su lugar se crea un **nodo concentrador** —una carpeta, una sesión, un
//! canónico— y se enlazan N−1 aristas contra él. El grafo queda legible, la
//! travesía acotada, y el nodo nuevo es además *conocimiento que el sistema
//! autoró* — justo lo que mide la métrica de §0 del maestro.

use obrain_core::{EntityId, Method, NewAssertion, Result, Store, TimePoint, Value};
use std::collections::HashMap;

pub mod duplicates;
pub mod provenance;
pub mod sessions;
pub mod structure;
pub mod versions;

/// Proyección en memoria de un archivo, montada una sola vez para que los
/// enlazadores no repitan consultas.
#[derive(Debug, Clone)]
pub struct FileView {
    pub entity: EntityId,
    pub path: String,
    pub hash: Option<String>,
    pub modified_ms: i64,
    pub size: u64,
    pub availability: String,
}

impl FileView {
    pub fn file_name(&self) -> &str {
        self.path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(&self.path)
    }

    pub fn parent(&self) -> &str {
        match self.path.rfind(['\\', '/']) {
            Some(i) => &self.path[..i],
            None => "",
        }
    }

    /// Sólo los archivos locales tienen bytes en disco. Leer un flujo alternativo
    /// de uno deshidratado podría provocar tráfico, así que no se toca.
    pub fn is_local(&self) -> bool {
        self.availability == "local"
    }
}

/// Carga todos los archivos del log en una sola pasada: cuatro consultas, no
/// cuatro por archivo.
pub fn load_files(store: &Store, at: TimePoint) -> Result<Vec<FileView>> {
    fn first_by_subject(
        rows: Vec<obrain_core::Assertion>,
    ) -> HashMap<EntityId, obrain_core::Value> {
        let mut m = HashMap::new();
        for a in rows {
            // `query` ordena por precedencia y luego por asserted_at DESC,
            // así que la primera de cada sujeto es la vigente más reciente.
            m.entry(a.subject).or_insert(a.object);
        }
        m
    }

    let paths = first_by_subject(store.query(None, Some("path_observed_at"), at)?);
    let hashes = first_by_subject(store.query(None, Some("has_hash"), at)?);
    let mtimes = first_by_subject(store.query(None, Some("modified_at"), at)?);
    let sizes = first_by_subject(store.query(None, Some("file_size"), at)?);
    let avail = first_by_subject(store.query(None, Some("availability"), at)?);

    let mut out = Vec::with_capacity(paths.len());
    for (entity, path) in paths {
        let Some(path) = path.as_text().map(str::to_string) else {
            continue;
        };
        out.push(FileView {
            hash: hashes.get(&entity).and_then(|v| v.as_text()).map(str::to_string),
            modified_ms: match mtimes.get(&entity) {
                Some(Value::Timestamp(t)) => *t,
                _ => 0,
            },
            size: match sizes.get(&entity) {
                Some(Value::Number(n)) => *n as u64,
                _ => 0,
            },
            availability: avail
                .get(&entity)
                .and_then(|v| v.as_text())
                .unwrap_or("local")
                .to_string(),
            entity,
            path,
        });
    }
    Ok(out)
}

#[derive(Debug, Default)]
pub struct LinkReport {
    pub duplicate_of: usize,
    pub contained_by: usize,
    pub downloaded_from: usize,
    pub occurred_during: usize,
    pub version_of: usize,

    /// Nodos concentradores creados: carpetas y sesiones.
    /// **Esto es conocimiento que el sistema autoró**, no que el usuario escribió.
    pub hubs_created: usize,
    /// Aristas que ya existían. Los enlazadores son idempotentes.
    pub already_present: usize,
    pub errors: Vec<String>,
}

impl LinkReport {
    pub fn edges_total(&self) -> usize {
        self.duplicate_of
            + self.contained_by
            + self.downloaded_from
            + self.occurred_during
            + self.version_of
    }
}

/// Una arista a punto de escribirse.
pub(crate) struct Edge<'a> {
    pub subject: &'a EntityId,
    pub predicate: &'a str,
    pub object: Value,
    pub method: Method,
    pub confidence: f64,
    /// Quién la produjo: `linker:hash`, `heuristic:filename-version`…
    /// Es lo que permite auditar y desactivar un enlazador concreto.
    pub source: &'a str,
}

/// Escribe una arista si no existe ya. Devuelve `true` si escribió.
pub(crate) fn write_edge(
    store: &Store,
    report: &mut LinkReport,
    edge: Edge<'_>,
    at: i64,
) -> Result<bool> {
    let tp = TimePoint::at(at);
    if store.has_assertion(edge.subject, edge.predicate, &edge.object, tp)? {
        report.already_present += 1;
        return Ok(false);
    }
    store.assert(
        NewAssertion::new(edge.subject.clone(), edge.predicate, edge.object, edge.method, at)
            .confidence(edge.confidence)
            .source(edge.source),
    )?;
    Ok(true)
}

/// Nombre legible de una entidad: su ruta, su carpeta, o el id si no tiene.
pub fn label(store: &Store, e: &EntityId, at: TimePoint) -> String {
    for p in ["path_observed_at", "folder_path"] {
        if let Ok(Some(a)) = store.resolve(e, p, at) {
            if let Some(t) = a.object.as_text() {
                return t.rsplit(['\\', '/']).next().unwrap_or(t).to_string();
            }
        }
    }
    if let Ok(Some(a)) = store.resolve(e, "session_span", at) {
        if let Value::Timestamp(t) = a.object {
            return format!("<sesion {t}>");
        }
    }
    format!("<{}>", &e.as_str()[..8.min(e.as_str().len())])
}

/// Muestra aristas reales de un predicado, para poder auditar el grafo en vez
/// de confiar en los contadores. Un enlazador que no se puede inspeccionar es
/// un enlazador en el que no se puede confiar.
pub fn sample_edges(
    store: &Store,
    predicate: &str,
    limit: usize,
    at: TimePoint,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for a in store.query(None, Some(predicate), at)?.into_iter().take(limit) {
        let from = label(store, &a.subject, at);
        let to = match &a.object {
            Value::Entity(e) => label(store, e, at),
            Value::Text(t) => t.chars().take(64).collect(),
            other => format!("{other:?}"),
        };
        out.push(format!("{from}  ->  {to}   [{:.2}]", a.confidence));
    }
    Ok(out)
}

/// Ejecuta todos los enlazadores. El orden importa poco salvo que la estructura
/// de carpetas se construye primero, porque crea nodos que los demás pueden usar.
pub fn link_all(store: &Store, files: &[FileView], at: i64) -> Result<LinkReport> {
    let mut report = LinkReport::default();
    structure::link(store, files, at, &mut report)?;
    duplicates::link(store, files, at, &mut report)?;
    provenance::link(store, files, at, &mut report)?;
    sessions::link(store, files, at, &mut report)?;
    versions::link(store, files, at, &mut report)?;
    Ok(report)
}
