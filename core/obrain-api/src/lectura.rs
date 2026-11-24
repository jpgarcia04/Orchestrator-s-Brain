//! **La apertura de la puerta de Aetheris**, con lo que sólo sabe este crate.
//!
//! [`obrain_core::Store::abrir_para_leer`] abre la base sin escribir nada
//! (ver su módulo). Aquí se le añaden las dos cosas que el núcleo no conoce:
//! el recibo de una restauración a medias, que es de [`crate::respaldo`], y
//! las tablas de las proyecciones (texto, vectores, ajustes de seguridad) que
//! las herramientas de la puerta van a leer.
//!
//! A diferencia de [`crate::Brain::open`], que **repara** una restauración
//! cortada, la puerta sólo la ve y se niega: reparar es escribir, y eso lo
//! hace la ventana (ADR-0038, «Lo que Brain rechaza»).

use crate::respaldo::RECIBO_TRANSICION_ARCHIVO;
use obrain_core::{Lectura, NoDisponible, Store};
use std::path::Path;

/// Las tablas de las proyecciones que la puerta lee además del log. Las crea
/// la ventana al abrir ([`crate::Brain::initialize_store`]); si falta alguna,
/// la base no está al día para esta versión.
pub const TABLAS_DE_LA_PUERTA: &[&str] = &[
    "extractions",
    "chunks",
    "chunks_fts",
    "vectors",
    "seguridad",
];

/// Abre `db` para una llamada de la puerta, o dice por qué no puede.
pub fn abrir_para_leer(db: &Path) -> Result<Lectura, NoDisponible> {
    let carpeta = std::path::absolute(db)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    if carpeta.is_some_and(|c| c.join(RECIBO_TRANSICION_ARCHIVO).exists()) {
        return Err(NoDisponible::Ocupada(
            "hay una restauración a medias; la termina la ventana al abrir".into(),
        ));
    }
    Store::abrir_para_leer(db, TABLAS_DE_LA_PUERTA)
}
