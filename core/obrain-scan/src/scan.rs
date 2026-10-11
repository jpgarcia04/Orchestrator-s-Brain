//! El recorrido: descubrir → identificar → triar.
//!
//! Orden deliberado. El triaje ocurre **antes** de abrir nada, usando sólo los
//! atributos que ya vienen en la enumeración del directorio. Un archivo que el
//! triaje marca como `CloudPlaceholder` no se toca jamás.

use crate::exclude::{ExclusionSet, SkipReason};
use crate::identity::{FileIdentity, content_hash, file_identity};
use crate::triage::{Availability, classify};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

/// Un archivo descubierto y clasificado, listo para ingestar.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub availability: Availability,
    pub size: u64,
    pub modified_ms: i64,
    /// `None` si el triaje prohibió abrirlo.
    pub identity: Option<FileIdentity>,
    /// `None` si el triaje prohibió leer contenido.
    pub hash: Option<String>,
}

/// Una carpeta recorrida, con su identidad ([ADR-0027]).
///
/// En NTFS un directorio tiene `FileID` como un archivo y lo conserva al
/// renombrarlo y al moverlo dentro del mismo disco: es lo que permite que una
/// carpeta marcada como sensible siga siéndolo con otro nombre.
///
/// [ADR-0027]: ../../../docs/adr/0027-la-carpeta-sigue-a-su-fileid.md
#[derive(Debug, Clone)]
pub struct FolderCandidate {
    pub path: PathBuf,
    pub identity: FileIdentity,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub dirs_visited: usize,
    /// Directorios podados. Los archivos que contienen **nunca se enumeran**,
    /// así que no aparecen en `files_seen`: ése es el objetivo de podar.
    pub dirs_pruned: usize,
    pub prune_reasons: BTreeMap<&'static str, usize>,
    pub files_seen: usize,
    pub files_skipped: usize,
    pub skip_reasons: BTreeMap<&'static str, usize>,
    pub by_availability: BTreeMap<&'static str, usize>,

    /// Cuántos archivos se abrieron (handle solicitado).
    pub files_opened: usize,
    /// Cuántos se leyeron enteros para hashear.
    pub files_hashed: usize,
    pub bytes_hashed: u64,
    /// **La métrica que valida M1**: archivos de nube que nunca se tocaron.
    pub cloud_untouched: usize,

    /// Las carpetas recorridas —la raíz incluida— con su identidad. Sólo las
    /// que el triaje deja abrir: una carpeta de nube que se descargaría al
    /// abrirla se queda sin identidad, como un archivo.
    pub folders: Vec<FolderCandidate>,

    pub errors: Vec<String>,
    pub elapsed: Duration,
}

impl ScanReport {
    fn count_skip(&mut self, r: SkipReason) {
        *self.skip_reasons.entry(r.as_str()).or_insert(0) += 1;
        self.files_skipped += 1;
    }
}

pub struct Scanner {
    pub exclusions: ExclusionSet,
    /// No hashear por encima de este tamaño en el primer paso; se difiere.
    pub max_hash_bytes: u64,
    /// Límite de archivos, para pruebas.
    pub limit: Option<usize>,
}

impl Default for Scanner {
    fn default() -> Self {
        Scanner {
            exclusions: ExclusionSet::new(),
            max_hash_bytes: 256 * 1024 * 1024,
            limit: None,
        }
    }
}

impl Scanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recorre `root` y devuelve los candidatos junto con el informe.
    pub fn scan(&self, root: &Path) -> (Vec<Candidate>, ScanReport) {
        let started = Instant::now();
        let mut report = ScanReport::default();
        let mut candidates = Vec::new();

        // La poda se contabiliza dentro del closure del walker, que es `FnMut`
        // pero también captura `&self`. `RefCell` mantiene ambas cosas posibles.
        let pruned: RefCell<BTreeMap<&'static str, usize>> = RefCell::new(BTreeMap::new());

        let walker = WalkDir::new(root)
            .follow_links(false) // los symlinks invitan a ciclos
            .into_iter()
            .filter_entry(|e| {
                // La raíz nunca se poda: el usuario la eligió a propósito.
                if e.depth() == 0 {
                    return true;
                }
                if !e.file_type().is_dir() {
                    return true;
                }
                let attrs = attributes_of(e.path());
                match self.exclusions.skip_dir(e.path(), attrs) {
                    Some(reason) => {
                        *pruned.borrow_mut().entry(reason.as_str()).or_insert(0) += 1;
                        false
                    }
                    None => true,
                }
            });

        for entry in walker {
            if let Some(limit) = self.limit {
                if candidates.len() >= limit {
                    break;
                }
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    report.errors.push(format!("{e}"));
                    continue;
                }
            };

            if entry.file_type().is_dir() {
                report.dirs_visited += 1;
                let attrs = entry.metadata().map(|m| attributes_from(&m)).unwrap_or(0);
                if classify(attrs).may_open() {
                    match file_identity(entry.path()) {
                        Ok(identity) => report.folders.push(FolderCandidate {
                            path: entry.path().to_path_buf(),
                            identity,
                        }),
                        Err(e) => report
                            .errors
                            .push(format!("{}: {e}", entry.path().display())),
                    }
                }
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            report.files_seen += 1;

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    report
                        .errors
                        .push(format!("{}: {e}", entry.path().display()));
                    continue;
                }
            };
            let attrs = attributes_from(&meta);
            let name = entry.file_name().to_string_lossy();

            if let Some(reason) = self.exclusions.skip_file(&name, attrs) {
                report.count_skip(reason);
                continue;
            }

            // ── TRIAJE. Todavía no se ha abierto nada. ──
            let availability = classify(attrs);
            *report
                .by_availability
                .entry(availability.as_str())
                .or_insert(0) += 1;

            let size = meta.len();
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            // Identidad: sólo si el triaje permite abrir.
            let identity = if availability.may_open() {
                match file_identity(entry.path()) {
                    Ok(id) => {
                        report.files_opened += 1;
                        Some(id)
                    }
                    Err(e) => {
                        report
                            .errors
                            .push(format!("{}: {e}", entry.path().display()));
                        None
                    }
                }
            } else {
                report.cloud_untouched += 1;
                None
            };

            candidates.push(Candidate {
                path: entry.path().to_path_buf(),
                availability,
                size,
                modified_ms,
                identity,
                hash: None,
            });
        }

        report.prune_reasons = pruned.into_inner();
        report.dirs_pruned = report.prune_reasons.values().sum();
        report.elapsed = started.elapsed();
        (candidates, report)
    }

    /// Hashea, **sólo** lo que el triaje permitió leer. Paralelo: es la parte
    /// cara del escaneo y está limitada por E/S y CPU, no por la base de datos.
    pub fn hash_all(&self, candidates: &mut [Candidate], report: &mut ScanReport) {
        use rayon::prelude::*;

        let max = self.max_hash_bytes;
        let results: Vec<(usize, std::io::Result<String>)> = candidates
            .par_iter()
            .enumerate()
            .filter(|(_, c)| c.availability.may_read_content() && c.size <= max)
            .map(|(i, c)| (i, content_hash(&c.path)))
            .collect();

        for (i, r) in results {
            match r {
                Ok(h) => {
                    report.files_hashed += 1;
                    report.bytes_hashed += candidates[i].size;
                    candidates[i].hash = Some(h);
                }
                Err(e) => report
                    .errors
                    .push(format!("{}: {e}", candidates[i].path.display())),
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────── atributos

#[cfg(windows)]
fn attributes_from(meta: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn attributes_from(meta: &std::fs::Metadata) -> u32 {
    if meta.is_dir() {
        crate::triage::attr::DIRECTORY
    } else {
        0
    }
}

/// Atributos de una ruta sin abrirla. `symlink_metadata` no sigue enlaces,
/// que es justo lo que hace falta para detectar junctions.
fn attributes_of(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .map(|m| attributes_from(&m))
        .unwrap_or(0)
}

/// Ahora en milisegundos desde epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
