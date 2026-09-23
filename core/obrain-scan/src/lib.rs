//! # obrain-scan — M1 · Ver el disco
//!
//! Descubrimiento, identidad y triaje de archivos.
//!
//! El criterio de "hecho" de M1 (`PROJECT-MASTER.md` §9) es exigente a
//! propósito: **indexar una máquina real sin hidratar un solo archivo de
//! OneDrive ni indexar `node_modules`.**
//!
//! El orden del pipeline no es casual:
//!
//! ```text
//!   descubrir  →  TRIAR  →  identificar  →  hashear  →  aseverar
//!                   ↑
//!         decide si se puede tocar,
//!         y ocurre ANTES de abrir nada
//! ```
//!
//! Un indexador que triaja después de abrir ya ha perdido: en Windows, abrir
//! un placeholder de OneDrive dispara su descarga. Por eso todo el módulo
//! [`triage`] trabaja sobre los atributos que ya vienen en la enumeración del
//! directorio, sin pedir un solo handle.

pub mod ausentes;
pub mod exclude;
pub mod identity;
pub mod ingest;
pub mod scan;
pub mod triage;

pub use exclude::{ExclusionSet, SkipReason};
pub use identity::FileIdentity;
pub use ingest::{IngestReport, ingest, library_path, register_library};
pub use scan::{Candidate, ScanReport, Scanner, now_ms};
pub use triage::Availability;
