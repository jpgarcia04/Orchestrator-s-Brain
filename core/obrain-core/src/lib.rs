//! # obrain-core — M0
//!
//! El log de aserciones bitemporal de Orchestrator's Brain.
//!
//! Este crate implementa el cimiento descrito en `docs/PROJECT-MASTER.md` §3:
//! la unidad primaria de datos **no es el nodo, es la aserción**. Un log
//! append-only de afirmaciones inmutables con tiempo bitemporal, procedencia,
//! confianza y evidencia. El grafo, el índice léxico y el vectorial son
//! proyecciones reconstruibles de ese log.
//!
//! ## Invariantes que este crate impone (no documenta: impone)
//!
//! - **I1** — el log es inmutable; sólo `retracted_at` puede cambiar.
//! - **I4** — nada se borra, se retracta.
//! - **I5** — las aserciones del usuario tienen precedencia permanente sobre
//!   cualquier inferencia. Es el invariante del que depende que este sistema
//!   merezca la confianza del usuario.
//! - **I6** — bitemporalidad: cuándo fue cierto ≠ cuándo se supo.
//!
//! I1 e I4 se imponen con triggers de SQLite, no con disciplina: ningún código
//! futuro puede violarlos, ni siquiera un `DELETE` escrito a mano.
//!
//! ## Ejemplo
//!
//! ```
//! use obrain_core::{Store, NewAssertion, Method, TimePoint};
//!
//! let store = Store::open_in_memory()?;
//! let doc = store.create_entity(1_000)?;
//!
//! // Un clasificador infiere algo, con confianza parcial.
//! store.assert(
//!     NewAssertion::new(doc.clone(), "about", "finanzas", Method::Inferred, 2_000)
//!         .confidence(0.6)
//!         .source("classifier@v1"),
//! )?;
//!
//! // El usuario corrige. A partir de aquí, ninguna inferencia lo desplaza.
//! store.assert(
//!     NewAssertion::new(doc.clone(), "about", "visa", Method::User, 3_000),
//! )?;
//!
//! let winner = store.resolve(&doc, "about", TimePoint::at(4_000))?.unwrap();
//! assert_eq!(winner.object.as_text(), Some("visa"));
//! # Ok::<(), obrain_core::Error>(())
//! ```

pub mod error;
pub mod model;
pub mod schema;
pub mod sensitivity;
pub mod store;
pub mod ubicacion;

/// Reexportado para que las proyecciones (índice léxico, vectorial) hablen con
/// la **misma** versión de `rusqlite` que el log. Dos versiones distintas del
/// mismo crate en el árbol serían dos tipos `Connection` incompatibles.
pub use rusqlite;

pub use error::{Error, Result};
pub use model::{
    Assertion, AssertionId, Cardinality, EntityId, Facet, Method, NewAssertion, PredicateDef,
    Sensitivity, TimePoint, Value,
};
pub use sensitivity::{CONTENTS_SENSITIVITY, FolderMark, FolderMarks, Origin, Resolution};
pub use store::{Contradiction, Store};
