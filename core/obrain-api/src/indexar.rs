//! **Indexar**: el pipeline entero en una llamada.
//!
//! Hasta aquí, meter una carpeta en el sistema eran cinco programas en orden
//! —`obrain-scan`, `obrain-link`, `obrain-extract`, `obrain-guard`,
//! `obrain-embed`— y una variable de entorno para que la aplicación encontrara
//! la base. Cada uno sigue existiendo y sigue siendo la herramienta para
//! **auditar** su etapa (`--sample`). Esto es la otra cosa: la que usa la
//! aplicación cuando pulsas «actualizar» y la que usa `obrain indexar`.
//!
//! No decide nada nuevo. Cada paso llama a la misma función que su CLI, con la
//! misma transacción, en el mismo orden:
//!
//! ```text
//! escanear → enlazar → extraer → clasificar → vencimientos → vectorizar
//!    M1        M3        M2      puerta (I11)    ADR-0023        M4
//! ```
//!
//! El orden no es arbitrario: la puerta clasifica con el texto que dejó M2, y
//! M4 sólo vectoriza lo que la puerta deja pasar. Los vencimientos también se
//! leen del texto de M2 —el de las fotos incluido— y no dependen de la puerta;
//! van antes de vectorizar porque son baratos y la vectorización no.
//!
//! **Es idempotente.** Volver a indexar una biblioteca sin cambios no escribe
//! nada: cada etapa compara con lo que ya sabía (ADR-0001). Por eso la
//! aplicación puede hacerlo al arrancar sin coste.

use crate::Result;
use obrain_core::{Store, TimePoint};
use obrain_embed::embedder::Embedder;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Lo que hizo una pasada, en las palabras que se le enseñan a una persona.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IndexReport {
    /// Las bibliotecas recorridas, con su ruta.
    pub libraries: Vec<String>,
    /// Bibliotecas registradas que ya no están en el disco (un USB
    /// desconectado, una carpeta movida). No se tocan: siguen en el log.
    pub missing: Vec<String>,
    /// Archivos vistos en el disco.
    pub files_seen: usize,
    /// De ésos, los que el sistema no conocía.
    pub files_new: usize,
    /// Relaciones nuevas.
    pub edges_new: usize,
    /// Documentos cuyo texto se leyó en esta pasada.
    pub documents_read: usize,
    /// De ésos, los que se leyeron con OCR: fotos y PDF escaneados.
    pub ocr: usize,
    /// Fragmentos nuevos en el índice de búsqueda.
    pub chunks_new: usize,
    /// Archivos que la puerta retiene como sensibles o de bóveda, en total: lo
    /// que dijo el clasificador, lo que corregiste y lo que hereda de una
    /// carpeta marcada.
    pub sensitive: usize,
    pub vault: usize,
    /// De ésos, los que lo son por estar dentro de una carpeta que marcaste.
    pub inherited: usize,
    /// Documentos con fecha de caducidad leída de su texto (ADR-0023).
    pub expiring: usize,
    /// Vectores nuevos. `0` con `semantic = false` si no había modelo.
    pub vectors_new: usize,
    /// ¿Se pasó por la etapa de significado?
    pub semantic: bool,
    pub elapsed_ms: u64,
}

/// Indexa estas carpetas y las deja registradas como **bibliotecas**.
///
/// `embedder` es opcional: sin modelo, todo menos la búsqueda por significado
/// queda al día, y los vectores que falten se escriben en la siguiente pasada
/// que sí lo tenga.
pub fn index(
    store: &mut Store,
    roots: &[PathBuf],
    embedder: Option<&dyn Embedder>,
) -> Result<IndexReport> {
    // El reconocedor del sistema: en Windows, el suyo; fuera, ninguno y las
    // fotos se saltan como siempre.
    let ocr = obrain_ocr::platform();
    index_with(store, roots, embedder, ocr.as_deref())
}

/// [`index`] con otro reconocedor de texto (o ninguno). Es la costura por la
/// que los tests meten un OCR de juguete.
pub fn index_with(
    store: &mut Store,
    roots: &[PathBuf],
    embedder: Option<&dyn Embedder>,
    ocr: Option<&dyn obrain_ocr::Ocr>,
) -> Result<IndexReport> {
    let started = Instant::now();
    let mut r = IndexReport::default();

    // ── M1 · escanear, una biblioteca cada vez ──
    for root in roots {
        let root = absoluta(root);
        let ruta = obrain_scan::library_path(&root);
        if !root.is_dir() {
            r.missing.push(ruta);
            continue;
        }
        let scanner = obrain_scan::Scanner::new();
        let (mut candidatos, mut informe) = scanner.scan(&root);
        scanner.hash_all(&mut candidatos, &mut informe);
        r.files_seen += informe.files_seen;

        let at = obrain_scan::now_ms();
        let mut ingesta = obrain_scan::IngestReport::default();
        store.transaction(|s| -> obrain_core::Result<()> {
            obrain_scan::ingest(s, &candidatos, at, &mut ingesta)?;
            obrain_scan::register_library(s, &root, at)?;
            Ok(())
        })?;
        r.files_new += ingesta.entities_created;
        r.libraries.push(ruta);
    }

    // ── M3 · enlazar ──
    let at = obrain_scan::now_ms();
    let archivos = obrain_link::load_files(store, TimePoint::at(at))?;
    let enlaces = store.transaction(|s| obrain_link::link_all(s, &archivos, at))?;
    r.edges_new = enlaces.edges_total();

    // ── M2 · extraer ──
    obrain_extract::index::migrate(store.conn())?;
    let at = obrain_scan::now_ms();
    let filas = obrain_extract::load_files(store, TimePoint::at(at))?;
    let mut extraccion = obrain_extract::ExtractReport::default();
    store.transaction(|s| obrain_extract::extract_all_with(s, &filas, at, &mut extraccion, ocr))?;
    r.documents_read = extraccion.extracted;
    r.ocr = extraccion.ocr;
    r.chunks_new = extraccion.chunks;

    // ── la puerta · clasificar ──
    // Se vuelven a leer las filas: la extracción acaba de escribir el texto
    // con el que clasifica la regla de contenido.
    let at = obrain_scan::now_ms();
    let filas = obrain_extract::load_files(store, TimePoint::at(at))?;
    let mut puerta = obrain_guard::GuardReport::default();
    store.transaction(|s| obrain_guard::classify_all(s, &filas, at, false, &mut puerta))?;
    r.sensitive = puerta.effective[1];
    r.vault = puerta.effective[2];
    r.inherited = puerta.inherited;

    // ── vencimientos ──
    let at = obrain_scan::now_ms();
    let mut vencimientos = obrain_expiry::ExpiryReport::default();
    store.transaction(|s| obrain_expiry::detect_all(s, at, &mut vencimientos))?;
    r.expiring = vencimientos.found;

    // ── M4 · vectorizar ──
    if let Some(modelo) = embedder {
        obrain_embed::index::migrate(store.conn())?;
        let at = obrain_scan::now_ms();
        let mut vectores = obrain_embed::EmbedReport::default();
        store.transaction(|s| obrain_embed::embed_all(s, modelo, at, &mut vectores))?;
        r.vectors_new = vectores.vectors_written;
        r.semantic = true;
    }

    r.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(r)
}

/// Vuelve a indexar **todas** las bibliotecas registradas. Es el «actualizar».
pub fn reindex(store: &mut Store, embedder: Option<&dyn Embedder>) -> Result<IndexReport> {
    let roots: Vec<PathBuf> = libraries(store)?.into_iter().map(PathBuf::from).collect();
    index(store, &roots, embedder)
}

/// Las rutas de las bibliotecas registradas.
pub fn libraries(store: &Store) -> Result<Vec<String>> {
    let at = TimePoint::at(obrain_scan::now_ms());
    Ok(obrain_link::libraries(store, at)?
        .into_iter()
        .map(|(_, ruta)| ruta)
        .collect())
}

/// Absoluta y sin resolver enlaces: `.\docs` y `C:\…\docs` tienen que ser la
/// misma biblioteca, porque la ruta de cada archivo **es** su carpeta para M3.
fn absoluta(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}
