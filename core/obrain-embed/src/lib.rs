//! # obrain-embed — M4 · Entender
//!
//! Embeddings locales, búsqueda semántica y fusión RRF. El criterio de "hecho"
//! del hito (`PROJECT-MASTER.md` §9) es **encontrar por significado, no por
//! nombre**: una consulta cuyas palabras no aparecen en el documento y que aun
//! así lo devuelve.
//!
//! ## Es el primer hito con un modelo dentro
//!
//! Y por eso es el hito de la decisión más difícil de deshacer. El invariante
//! I7 lo dice sin rodeos: *los embeddings son el lock-in real, no el LLM*. Un
//! LLM se cambia editando una llamada; un índice vectorial de dos años de
//! antigüedad sin saber qué modelo lo produjo no se cambia, se tira. Por eso
//! cada vector guarda `(model_id, model_version, dim, chunker, chunker_ver)`
//! **en su clave primaria** ([`index`]), y por eso dos modelos pueden convivir
//! en la misma tabla mientras dure una migración.
//!
//! ## El orden, otra vez
//!
//! ```text
//!   gate::embeddable  →  fragmentos  →  Embedder  →  vectors  →  kNN
//!          ↑                                                      ↓
//!   la puerta decide QUIÉN entra                          BM25 ⊕ kNN → RRF
//!   (I11), no `SELECT DISTINCT entity FROM chunks`
//! ```
//!
//! La primera flecha es la que importa. `SELECT DISTINCT entity FROM chunks`
//! devuelve casi lo mismo —difieren en seis archivos de 379— y es exactamente
//! la diferencia entre cumplir I11 y decir que se cumple. La puerta va antes,
//! y además cada invocación del modelo pasa por
//! [`obrain_guard::gate::check`].
//!
//! ## Qué es verdad y qué es índice
//!
//! Los vectores son una **proyección** (I1): tabla propia, reconstruible, se
//! puede tirar entera. Al log va sólo la procedencia —qué modelo, qué versión,
//! cuántos vectores—, igual que hizo M2 con la extracción.

pub mod download;
pub mod e5;
pub mod embedder;
pub mod fuse;
pub mod index;
pub mod search;

use embedder::{Embedder, Role};
use obrain_core::{Method, NewAssertion, Store, TimePoint, Value};
use obrain_guard::gate::{self, Capability, Runs};
use std::time::{Duration, Instant};

pub use embedder::{ModelId, Role as EmbedRole};
pub use search::Hit;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] obrain_core::Error),

    #[error("sqlite: {0}")]
    Sqlite(#[from] obrain_core::rusqlite::Error),

    #[error("onnx runtime: {0}")]
    Ort(#[from] ort::Error),

    #[error("e/s: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "falta un archivo del modelo: {0}\n\
         Ejecuta `obrain-embed <db> --descargar-modelo` una vez; \
         despues todo el pipeline funciona sin conexion."
    )]
    ModelMissing(String),

    #[error("modelo: {0}")]
    Model(String),
}

/// Lo que hizo una pasada de vectorización.
#[derive(Debug, Default)]
pub struct EmbedReport {
    /// Entidades con texto que la puerta **concede**.
    pub entities_allowed: usize,
    /// Entidades con texto que la puerta **retiene**. El coste de la política.
    pub entities_withheld: usize,
    /// Fragmentos retenidos por la puerta.
    pub chunks_withheld: i64,
    /// Fragmentos que hacía falta vectorizar.
    pub pending: usize,
    pub vectors_written: usize,
    /// Entidades cuyos vectores se han sacado del índice porque subieron a
    /// bóveda después de vectorizarse (una carpeta marcada, una regla nueva).
    pub entities_purged: usize,
    /// Entidades que la puerta rechazó **en la comprobación por invocación**.
    /// Debe ser 0: si no lo es, `embeddable` y `check` discrepan y eso es un
    /// defecto del control, no una estadística.
    pub denied_at_call: usize,
    pub assertions_written: usize,
    pub assertions_unchanged: usize,
    pub elapsed: Duration,
    pub model_load: Duration,
    /// Sólo el tiempo dentro del modelo.
    pub inference: Duration,
    pub tokens_est: usize,
}

/// Vectoriza todo lo pendiente. El llamante debe haber migrado el esquema.
///
/// La puerta se consulta **dos veces** a propósito:
///
/// 1. [`gate::embeddable`] decide el conjunto de partida.
/// 2. [`gate::check`] se llama por entidad, justo antes de invocar el modelo.
///
/// Parece redundante y no lo es. La primera es una consulta de conjunto que
/// alguien podría cambiar por comodidad dentro de dos años; la segunda es la
/// que se ejecuta pegada a la invocación y no se puede saltar sin que se note.
/// `denied_at_call` mide si las dos están de acuerdo: si algún día no lo
/// están, el número lo dice en vez de que el desacuerdo pase inadvertido.
pub fn embed_all(
    store: &Store,
    embedder: &dyn Embedder,
    at: i64,
    report: &mut EmbedReport,
) -> Result<()> {
    let started = Instant::now();
    let model = embedder.model();
    let tp = TimePoint::at(at);

    // ── 0 · quién ya no puede estar ──
    // Lo que subió a bóveda después de vectorizarse sale del índice, y su
    // procedencia deja de estar vigente: el log no puede seguir afirmando que
    // está vectorizado algo que ya no lo está.
    for e in index::purge_withheld(store.conn())? {
        for p in ["embedded_with", "vector_count"] {
            for a in store.query(Some(&e), Some(p), tp)? {
                store.retract(&a.id, at)?;
            }
        }
        report.entities_purged += 1;
    }

    // ── 1 · quién entra. La puerta, no `chunks`. ──
    let allowed = gate::embeddable(store.conn())?;
    report.entities_allowed = allowed.len();
    report.chunks_withheld = gate::withheld_chunks(store.conn())?;
    let with_text: i64 = store.conn().query_row(
        "SELECT COUNT(DISTINCT entity) FROM chunks",
        [],
        |r| r.get(0),
    )?;
    report.entities_withheld = (with_text as usize).saturating_sub(allowed.len());

    // ── 2 · qué falta ──
    let pending = index::pending(store.conn(), &allowed, model)?;
    report.pending = pending.len();

    // ── 3 · vectorizar por lotes ──
    let batch = embedder.batch_size();
    for group in pending.chunks(batch) {
        // Segunda comprobación, pegada a la invocación.
        let mut cleared = Vec::with_capacity(group.len());
        for p in group {
            if gate::check(store, &p.entity, Capability::Embedder, Runs::Local)?.allowed() {
                cleared.push(p);
            } else {
                report.denied_at_call += 1;
            }
        }
        if cleared.is_empty() {
            continue;
        }
        let texts: Vec<&str> = cleared.iter().map(|p| p.text.as_str()).collect();
        report.tokens_est += texts.iter().map(|t| t.len() / 4).sum::<usize>();

        let t0 = Instant::now();
        let vectors = embedder.embed(&texts, Role::Passage)?;
        report.inference += t0.elapsed();

        for (p, v) in cleared.iter().zip(vectors) {
            index::insert(store.conn(), p, model, &v)?;
            report.vectors_written += 1;
        }
    }

    // ── 4 · procedencia al log ──
    // Sólo lo que no se puede recalcular mirando la tabla: con qué modelo se
    // vectorizó cada entidad y cuántos vectores salieron. El vector no va al
    // log: es proyección (I1, ADR-0005).
    for e in &allowed {
        let n: i64 = store.conn().query_row(
            "SELECT COUNT(*) FROM vectors WHERE entity = ?1 AND model_id = ?2 AND model_ver = ?3",
            obrain_core::rusqlite::params![e.as_str(), model.id, model.version],
            |r| r.get(0),
        )?;
        if n == 0 {
            continue;
        }
        write_if_changed(store, report, e, "embedded_with", Value::Text(model.label()), tp, at)?;
        write_if_changed(store, report, e, "vector_count", Value::Number(n as f64), tp, at)?;
    }

    report.elapsed = started.elapsed();
    Ok(())
}

/// Igual que en M1 y M2: reafirmar lo que ya se sabe infla el log sin aportar
/// información (ADR-0001).
fn write_if_changed(
    store: &Store,
    report: &mut EmbedReport,
    subject: &obrain_core::EntityId,
    predicate: &str,
    value: Value,
    tp: TimePoint,
    at: i64,
) -> Result<()> {
    if let Some(current) = store.resolve(subject, predicate, tp)? {
        if current.object == value {
            report.assertions_unchanged += 1;
            return Ok(());
        }
    }
    store.assert(
        NewAssertion::new(subject.clone(), predicate, value, Method::Deterministic, at)
            .source("embed@m4"),
    )?;
    report.assertions_written += 1;
    Ok(())
}
