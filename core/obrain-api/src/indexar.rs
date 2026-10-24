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
//! escanear → enlazar → extraer → clasificar → vencimientos → vectorizar → olvidar
//!    M1        M3        M2      puerta (I11)    ADR-0023        M4       ADR-0025
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

use crate::{Result, publication};
use obrain_core::{Store, TimePoint};
use obrain_embed::embedder::Embedder;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

/// Lo que hizo una pasada, en las palabras que se le enseñan a una persona.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct IndexReport {
    /// Las bibliotecas recorridas, con su ruta.
    pub libraries: Vec<String>,
    /// Bibliotecas registradas que ya no están en el disco (un USB
    /// desconectado, una carpeta movida). No se tocan: siguen en el log.
    pub missing: Vec<String>,
    /// El recorrido no fue fiable; no se escribió ni se publicó una pasada parcial.
    pub errors: Vec<String>,
    /// La pasada se omitió deliberadamente porque la reindexación está en pausa
    /// preventiva (contiene el motivo registrado, p. ej. tras una restauración).
    /// No representa un fallo de lectura ni ausencia de bibliotecas.
    pub paused: Option<String>,
    /// Archivos cuya identidad no bastó para decidir. Se reintentan al actualizar.
    pub deferred: Vec<String>,
    /// Archivos vistos en el disco.
    pub files_seen: usize,
    /// De ésos, los que el sistema no conocía.
    pub files_new: usize,
    /// Documentos que ya no están en el disco y pasan a la papelera
    /// (ADR-0025).
    pub trashed: usize,
    /// Documentos de la papelera que han vuelto.
    pub returned: usize,
    /// Carpetas que se renombraron o se movieron y siguen siendo las mismas,
    /// con su marca (ADR-0027).
    pub folders_moved: usize,
    /// Documentos de la papelera que se olvidaron en esta pasada, por llevar
    /// ahí 30 días.
    pub forgotten: usize,
    /// Relaciones nuevas.
    pub edges_new: usize,
    /// Documentos cuyo texto se leyó en esta pasada.
    pub documents_read: usize,
    /// De ésos, los que se leyeron con OCR: fotos y PDF escaneados.
    pub ocr: usize,
    /// Fragmentos nuevos en el índice de búsqueda.
    pub chunks_new: usize,
    /// Documentos cuya nueva versión aún no se pudo publicar.
    pub pending: usize,
    /// Archivos que la puerta retiene como sensibles o de bóveda, en total: lo
    /// que dijo el clasificador, lo que corregiste y lo que hereda de una
    /// carpeta marcada.
    pub sensitive: usize,
    pub vault: usize,
    /// De ésos, los que lo son por estar dentro de una carpeta que marcaste.
    pub inherited: usize,
    /// Y los que lo son por lo que llevan dentro (ADR-0030).
    pub from_parts: usize,
    /// Documentos que están dentro de otro: los estados de cuenta dentro del
    /// PDF de la visa (ADR-0030).
    pub inside: usize,
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
    let mut r = IndexReport {
        pending: store.content_pending_set()?.len(),
        ..Default::default()
    };
    if crate::seguridad::reindexado_pausado(store.conn()).unwrap_or(false) {
        let motivo = crate::seguridad::motivo_reindexado_pausado(store.conn())
            .ok()
            .flatten()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "restauración reciente".to_string());
        r.paused = Some(motivo);
        r.elapsed_ms = started.elapsed().as_millis() as u64;
        return Ok(r);
    }
    // Las carpetas recorridas de todas las bibliotecas, con su identidad: se
    // apuntan después de enlazar, cuando el enlazador ya ha creado las nuevas.
    let mut carpetas: Vec<obrain_scan::FolderCandidate> = Vec::new();

    // M1 tiene dos fases: inventariar todas las bibliotecas y sólo después
    // reconciliar. Un `indexar <nueva>` también incluye las ya registradas.
    let mut all_roots: Vec<PathBuf> = libraries(store)?.into_iter().map(PathBuf::from).collect();
    all_roots.extend(roots.iter().map(|r| absoluta(r)));
    let mut unique = HashSet::new();
    all_roots.retain(|r| unique.insert(obrain_scan::library_path(r)));
    let mut observations: Vec<obrain_scan::Candidate> = Vec::new();
    let mut candidate_paths: HashMap<PathBuf, usize> = HashMap::new();
    for root in all_roots {
        let ruta = obrain_scan::library_path(&root);
        if !root.is_dir() {
            r.missing.push(ruta);
            continue;
        }
        let scanner = obrain_scan::Scanner::new();
        let (mut candidatos, mut informe) = scanner.scan(&root);
        scanner.hash_all(&mut candidatos, &mut informe);
        r.files_seen += informe.files_seen;
        if !informe.complete && informe.errors.is_empty() {
            r.errors
                .push(format!("{}: recorrido parcial", root.display()));
        }
        r.errors.extend(informe.errors);
        // Raíces solapadas producen la misma ruta dos veces. Dos FileID
        // distintos para la misma ruta significan que cambió durante la pasada.
        for c in candidatos {
            if let Some(index) = candidate_paths.get(&c.path) {
                let previous = &observations[*index];
                if previous.identity != c.identity
                    || previous.size != c.size
                    || previous.modified_ms != c.modified_ms
                    || previous.hash != c.hash
                    || previous.availability != c.availability
                {
                    r.errors
                        .push(format!("{} cambió durante el recorrido", c.path.display()));
                }
            } else {
                candidate_paths.insert(c.path.clone(), observations.len());
                observations.push(c);
            }
        }
        carpetas.append(&mut informe.folders);
        r.libraries.push(ruta);
    }
    r.files_seen = observations.len();
    // Una biblioteca ausente o una lectura fallida impide afirmar que el FileID
    // antiguo desapareció. La pasada queda aplazada sin tocar el log.
    if !r.missing.is_empty() || !r.errors.is_empty() {
        r.elapsed_ms = started.elapsed().as_millis() as u64;
        return Ok(r);
    }
    let mut folder_paths = HashSet::new();
    carpetas.retain(|folder| folder_paths.insert(folder.path.clone()));
    // Verificación optimista fuera de la transacción: un rename entre el
    // inventario y el commit se reintentará en la siguiente pasada.
    for c in &observations {
        if let Err(error) = verify_candidate(c) {
            r.errors.push(error);
        }
    }
    if !r.errors.is_empty() {
        r.elapsed_ms = started.elapsed().as_millis() as u64;
        return Ok(r);
    }
    let mut absent = Vec::new();
    for root in &r.libraries {
        let root = Path::new(root);
        if !root.is_dir() {
            r.errors.push(format!(
                "{} dejó de estar disponible durante el recorrido",
                root.display()
            ));
            break;
        }
        absent.extend(obrain_scan::ausentes::observe_missing(
            store,
            root,
            obrain_scan::now_ms(),
        )?);
    }
    if !r.errors.is_empty() {
        r.elapsed_ms = started.elapsed().as_millis() as u64;
        return Ok(r);
    }

    let at = obrain_scan::now_ms();
    let mut ingesta = obrain_scan::IngestReport::default();
    let (ausentes, mudadas) = store.transaction(|s| -> obrain_core::Result<(usize, usize)> {
        obrain_scan::ingest(s, &observations, at, &mut ingesta)?;
        for root in roots {
            obrain_scan::register_library(s, &absoluta(root), at)?;
        }
        // Una carpeta que el recorrido ve con su `FileID` en otra ruta se
        // renombró o se movió: se muda antes de enlazar, para que el
        // enlazador la encuentre y no cree otra (ADR-0027).
        let mudadas = obrain_scan::carpetas::follow(s, &carpetas, at)?;
        // Lo que el log cree que hay aquí y el disco dice que ya no existe
        // pasa a la papelera (ADR-0025). Después de hashear: el hueco de un
        // guardado atómico ya se ha cerrado.
        let ausentes = if ingesta.deferred.is_empty() {
            obrain_scan::ausentes::apply_missing(s, &absent, &ingesta.seen, at)?
        } else {
            obrain_scan::ausentes::hold_missing(s, &absent, &ingesta.seen, at)?;
            0
        };
        Ok((ausentes, mudadas))
    })?;
    r.folders_moved = mudadas;
    r.files_new = ingesta.entities_created;
    r.trashed = ausentes;
    r.returned = ingesta.entities_returned;
    r.deferred = ingesta.deferred;

    // ── M3 · enlazar ──
    let at = obrain_scan::now_ms();
    let archivos = obrain_link::load_files(store, TimePoint::at(at))?;
    let enlaces = store.transaction(|s| -> obrain_core::Result<_> {
        let enlaces = obrain_link::link_all(s, &archivos, at)?;
        obrain_scan::carpetas::identify(s, &carpetas, at)?;
        Ok(enlaces)
    })?;
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
    r.errors.extend(extraccion.errors);

    // Las relaciones de contenido, la clasificación efectiva y la publicación
    // comparten un commit. Ningún lector ve una relación nueva con sensibilidad
    // antigua ni un chunk nuevo que todavía no haya pasado por la puerta.
    let at = obrain_scan::now_ms();
    let mut dentro = obrain_link::dentro::DentroReport::default();
    let mut puerta = obrain_guard::GuardReport::default();
    store.transaction(|s| -> Result<()> {
        obrain_link::dentro::link(s, at, &mut dentro)?;
        let filas = obrain_extract::load_files(s, TimePoint::at(at))?;
        obrain_guard::classify_all(s, &filas, at, false, &mut puerta)?;
        publication::publish_pending(s, &filas, ocr.is_some(), None, at)?;
        Ok(())
    })?;
    r.inside = dentro.encontrados;
    r.sensitive = puerta.effective[1];
    r.vault = puerta.effective[2];
    r.inherited = puerta.inherited;
    r.from_parts = puerta.from_parts;
    r.pending = store.content_pending_set()?.len();

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

    // ── la papelera · olvidar lo que lleva ahí 30 días ──
    let at = obrain_scan::now_ms();
    r.forgotten = store.transaction(|s| crate::papelera::expire(s, at))?;

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

fn verify_candidate(c: &obrain_scan::Candidate) -> std::result::Result<(), String> {
    let changed = || format!("{} cambió durante el recorrido", c.path.display());
    let meta = std::fs::symlink_metadata(&c.path).map_err(|e| format!("{}: {e}", changed()))?;
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if !meta.is_file()
        || meta.len() != c.size
        || (c.modified_ms != 0 && modified_ms != c.modified_ms)
    {
        return Err(changed());
    }
    if let Some(id) = c.identity {
        if obrain_scan::identity::file_identity(&c.path).ok() != Some(id) {
            return Err(changed());
        }
    }
    Ok(())
}

#[cfg(test)]
mod inventory_tests {
    use super::*;

    #[test]
    fn reemplazo_durante_el_recorrido_invalida_el_inventario() {
        let dir = std::env::temp_dir().join(format!(
            "obrain-race-{}-{}",
            std::process::id(),
            obrain_scan::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.txt");
        std::fs::write(&path, "antes").unwrap();
        let (candidates, _) = obrain_scan::Scanner::new().scan(&dir);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "despues y con otro tamaño").unwrap();
        assert!(verify_candidate(&candidates[0]).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
