//! # obrain-extract — M2 · Leer el disco
//!
//! Extracción de texto por formato, fragmentación versionada e índice léxico
//! FTS5. El criterio de "hecho" del hito (`PROJECT-MASTER.md` §9) es
//! **búsqueda léxica instantánea sobre el corpus completo**.
//!
//! ## El orden del pipeline vuelve a ser lo importante
//!
//! ```text
//!   formato por extension  →  TRIAJE  →  abrir  →  extraer  →  normalizar
//!                                ↑                                  ↓
//!                    ¿hay bytes en disco?                      fragmentar
//!                    (I1 de M1: nunca hidratar)                     ↓
//!                                                              FTS5 + log
//! ```
//!
//! M1 estableció que **triar va antes de abrir**. M2 hereda esa regla entera y
//! le añade una consecuencia incómoda: en esta máquina, los 14 documentos de
//! Office del corpus están *todos* deshidratados en OneDrive. Extraer su texto
//! exigiría descargarlos. Así que no se extraen. Un indexador que "sólo esta
//! vez" abre un placeholder no es un indexador con una excepción: es el
//! indexador que baja 40 GB sin avisar.
//!
//! ## Qué es verdad y qué es índice
//!
//! El texto extraído y sus fragmentos viven en **tablas propias** (`chunks`,
//! `chunks_fts`), no en el log. Son una proyección reconstruible desde los
//! archivos, que es exactamente lo que I1 dice que debe ser un índice. Al log
//! va sólo el **hecho** de haber extraído —extractor, chunker, tamaño, hash del
//! texto—, que es procedencia y no se puede recalcular si el archivo cambia.
//! El razonamiento largo está en
//! [`docs/adr/0005`](../../../docs/adr/0005-donde-vive-el-texto-extraido.md).

pub mod chunk;
pub mod evidence;
pub mod format;
pub mod index;
pub mod noise;
pub mod search;
pub mod text;

use obrain_core::{
    Assertion, EntityId, Facet, Method, NewAssertion, Result, Store, TimePoint, Value,
};
use obrain_scan::Availability;
use std::collections::{BTreeMap, HashMap};

pub use format::Format;
pub use search::{Hit, search};

/// Techo de tamaño para abrir un archivo. Se decide **con el tamaño que ya
/// está en el log**, sin tocar el disco.
///
/// No es una optimización: un `.log` de 400 MB de una aplicación produciría
/// 300.000 fragmentos que enterrarían el corpus entero en BM25.
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// En qué acabó el intento de extraer un archivo.
///
/// Los cinco primeros valores se deciden **sin abrir nada**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// No sabemos leer este formato. Derivable de la ruta.
    Unsupported,
    /// Los bytes no están en disco. Abrirlo dispararía una descarga.
    NotLocal,
    /// Más grande que [`MAX_FILE_BYTES`].
    TooLarge,
    /// Ya extraído, con el mismo hash de archivo y las mismas versiones.
    Unchanged,
    /// Se leyó y no era texto (un LevelDB llamado `.log`, un `.dat` de partida).
    Binary,
    /// Es texto, pero lo generó una máquina para una máquina. Ver [`noise`].
    Minified,
    /// Es texto, pero es un diccionario: `passwords.txt`, `surnames.txt`.
    WordList,
    /// Se leyó, se extrajo y salió texto.
    Ok,
    /// Se leyó y se extrajo cero texto. El caso canónico es un **PDF escaneado**:
    /// tiene páginas, pero son imágenes. No es un error.
    Empty,
    /// El extractor falló o entró en pánico con este archivo.
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Unsupported => "unsupported",
            Status::NotLocal => "not_local",
            Status::TooLarge => "too_large",
            Status::Unchanged => "unchanged",
            Status::Binary => "binary",
            Status::Minified => "minified",
            Status::WordList => "wordlist",
            Status::Ok => "ok",
            Status::Empty => "empty",
            Status::Failed => "failed",
        }
    }

    /// ¿Aporta este resultado información que no esté ya en el log?
    ///
    /// `unsupported` se deduce de la ruta y `not_local` de la aserción
    /// `availability` que ya escribió M1. Reafirmarlos para 900 archivos en
    /// cada pasada inflaría el log sin añadir nada — que es justo lo que
    /// ADR-0001 pide evitar. Que un `.log` resultara ser binario, en cambio,
    /// sólo se sabe abriéndolo.
    pub fn worth_asserting(self) -> bool {
        matches!(
            self,
            Status::Ok
                | Status::Empty
                | Status::Failed
                | Status::Binary
                | Status::Minified
                | Status::WordList
        )
    }
}

/// Proyección de un archivo del log, con lo justo para decidir si se toca.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub entity: EntityId,
    pub path: String,
    pub size: u64,
    /// Tal y como la escribió M1. Se reinterpreta con
    /// [`Availability::parse`], que falla cerrado ante un valor desconocido.
    pub availability: String,
    pub file_hash: Option<String>,
}

impl FileRow {
    pub fn file_name(&self) -> &str {
        self.path.rsplit(['\\', '/']).next().unwrap_or(&self.path)
    }

    /// **La puerta.** Todo lo que abre un archivo pasa por aquí.
    ///
    /// Una etiqueta que no reconocemos devuelve `false`: ante la duda no se
    /// toca, porque el coste de equivocarse es una descarga de OneDrive.
    pub fn may_read_content(&self) -> bool {
        Availability::parse(&self.availability).is_some_and(Availability::may_read_content)
    }
}

/// Carga los archivos del log en una pasada: cuatro consultas, no cuatro por
/// archivo.
pub fn load_files(store: &Store, at: TimePoint) -> Result<Vec<FileRow>> {
    fn first(rows: Vec<Assertion>) -> HashMap<EntityId, Value> {
        let mut m = HashMap::new();
        for a in rows {
            m.entry(a.subject).or_insert(a.object);
        }
        m
    }

    let paths = first(store.query(None, Some("path_observed_at"), at)?);
    let sizes = first(store.query(None, Some("file_size"), at)?);
    let avail = first(store.query(None, Some("availability"), at)?);
    let hashes = first(store.query(None, Some("has_hash"), at)?);

    let mut out = Vec::with_capacity(paths.len());
    for (entity, path) in paths {
        let Some(path) = path.as_text().map(str::to_string) else {
            continue;
        };
        out.push(FileRow {
            size: match sizes.get(&entity) {
                Some(Value::Number(n)) => *n as u64,
                _ => 0,
            },
            availability: avail
                .get(&entity)
                .and_then(|v| v.as_text())
                // Sin etiqueta no hay permiso: `parse` de una cadena vacía es
                // `None` y `may_read_content` será `false`.
                .unwrap_or_default()
                .to_string(),
            file_hash: hashes
                .get(&entity)
                .and_then(|v| v.as_text())
                .map(str::to_string),
            entity,
            path,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────── informe

#[derive(Debug, Default, Clone, Copy)]
pub struct FormatStat {
    pub files: usize,
    pub chunks: usize,
    pub chars: u64,
    pub empty: usize,
    pub failed: usize,
    pub noise: usize,
}

#[derive(Debug, Default)]
pub struct ExtractReport {
    /// Archivos que había en el log al empezar.
    pub considered: usize,

    // ── decidido sin abrir nada ──
    pub skipped_unsupported: usize,
    pub skipped_not_local: usize,
    pub skipped_too_large: usize,
    pub unchanged: usize,

    // ── de los que sí se abrieron ──
    pub opened: usize,
    pub extracted: usize,
    pub empty: usize,
    pub binary: usize,
    /// Texto legítimo que aun así no entra: minificado o diccionario.
    pub noise: usize,
    pub failed: usize,
    /// De los extraídos, cuántos se leyeron con OCR: fotos y PDF escaneados.
    pub ocr: usize,

    pub bytes_read: u64,
    pub chars: u64,
    pub chunks: usize,
    pub assertions_written: usize,
    pub assertions_unchanged: usize,
    pub by_format: BTreeMap<&'static str, FormatStat>,
    pub errors: Vec<String>,
}

impl ExtractReport {
    /// Las dos identidades que tiene que cumplir el informe.
    ///
    /// Existe porque en M1 la comprobación de que `35.583 − 33.517 = 2.066`
    /// fue lo que dio confianza en los números. Un contador que no cierra es
    /// un contador que miente, y se detecta gratis.
    pub fn arithmetic_holds(&self) -> bool {
        let sin_abrir = self.skipped_unsupported
            + self.skipped_not_local
            + self.skipped_too_large
            + self.unchanged;
        let abiertos = self.extracted + self.empty + self.binary + self.noise + self.failed;
        sin_abrir + self.opened == self.considered && abiertos == self.opened
    }
}

// ─────────────────────────────────────────────────────────────────── pipeline

/// Extrae, fragmenta e indexa todo el corpus. El llamante envuelve en
/// transacción.
///
/// Sin OCR: las fotos se saltan y un PDF escaneado sale vacío, como siempre.
pub fn extract_all(
    store: &Store,
    files: &[FileRow],
    at: i64,
    report: &mut ExtractReport,
) -> Result<()> {
    extract_all_with(store, files, at, report, None)
}

/// [`extract_all`] con un reconocedor de texto para lo que es una imagen.
///
/// Con OCR, dos cosas cambian:
///
/// - **Las fotos se leen.** La TIE fotografiada, la receta, el escaneo en JPG.
/// - **Un PDF que sale vacío se vuelve a intentar con OCR**, y los que ya
///   estaban anotados como vacíos por una pasada sin OCR se reintentan una
///   vez: se sabe porque su extractor era `pdf` y no `pdf+ocr`.
///
/// El texto reconocido entra en el mismo índice que el resto —es una
/// proyección del archivo como cualquier otra (ADR-0005)— y el log anota quién
/// lo leyó (`ocr_engine`), porque un texto reconocido puede tener errores y eso
/// tiene que poder decirse.
pub fn extract_all_with(
    store: &Store,
    files: &[FileRow],
    at: i64,
    report: &mut ExtractReport,
    ocr: Option<&dyn obrain_ocr::Ocr>,
) -> Result<()> {
    index::migrate(store.conn())?;
    report.considered = files.len();
    let tp = TimePoint::at(at);
    let (chunker, chunker_ver) = (chunk::CHUNKER, chunk::CHUNKER_VERSION);

    for file in files {
        // ── 1 · ¿sabemos leer este formato? Pura ruta, cero disco. ──
        let Some(format) = format::detect(&file.path) else {
            report.skipped_unsupported += 1;
            continue;
        };
        // Una imagen sin reconocedor es, a efectos prácticos, un formato que
        // no se sabe leer: lo mismo que era antes de que hubiera OCR.
        if format == Format::Image && ocr.is_none() {
            report.skipped_unsupported += 1;
            continue;
        }

        // ── 2 · TRIAJE. Antes de abrir. Sin excepciones. ──
        if !file.may_read_content() {
            report.skipped_not_local += 1;
            continue;
        }

        // ── 3 · techo de tamaño, con el dato que ya estaba en el log ──
        if file.size > MAX_FILE_BYTES {
            report.skipped_too_large += 1;
            continue;
        }

        // ── 4 · ¿cambió algo desde la última vez? ──
        let prev = index::previous(store.conn(), &file.entity, chunker, chunker_ver)?;
        let unchanged = prev.as_ref().is_some_and(|p| {
            p.extractor_ver == format.version()
                && p.file_hash.is_some()
                && p.file_hash == file.file_hash
                // Un PDF escaneado que se dio por vacío sin OCR, ahora que hay
                // OCR, se vuelve a mirar. Una vez: después su extractor ya es
                // `pdf+ocr`.
                && !(ocr.is_some() && format == Format::Pdf && p.status == "empty" && p.extractor == "pdf")
        });
        if unchanged {
            report.unchanged += 1;
            continue;
        }

        // ── 5 · a partir de aquí sí se tocan los bytes ──
        report.opened += 1;
        let bytes = match std::fs::read(&file.path) {
            Ok(b) => b,
            Err(e) => {
                report.failed += 1;
                report.errors.push(format!("{}: {e}", file.file_name()));
                write_outcome(
                    store,
                    report,
                    file,
                    format.as_str(),
                    Status::Failed,
                    None,
                    None,
                    0,
                    0,
                    at,
                    tp,
                )?;
                continue;
            }
        };
        report.bytes_read += bytes.len() as u64;

        let stat = report.by_format.entry(format.as_str()).or_default();
        stat.files += 1;

        // Una imagen, directamente al reconocedor; el resto, a su extractor.
        let extraido = match (format, ocr) {
            (Format::Image, Some(o)) => o.image(&bytes),
            _ => format.extract(&bytes),
        };
        let mut leido_con: Option<String> = (format == Format::Image)
            .then(|| ocr.map(|o| o.engine()))
            .flatten();
        let mut extractor = format.as_str();
        let raw = match extraido {
            Ok(t) => t,
            // El lector de PDF no pudo con él. Un escaneo raro también hace
            // eso, y con OCR se puede leer igual.
            Err(detail) if format == Format::Pdf && ocr.is_some() && detail != "binario" => {
                extractor = "pdf+ocr";
                match ocr.and_then(|o| o.pdf(&bytes, obrain_ocr::MAX_PDF_PAGES)) {
                    Some(Ok(t)) => {
                        leido_con = ocr.map(|o| o.engine());
                        t
                    }
                    otro => {
                        let por_que = match otro {
                            Some(Err(e)) => format!("{detail}; {e}"),
                            _ => detail,
                        };
                        report.failed += 1;
                        report
                            .errors
                            .push(format!("{}: {por_que}", file.file_name()));
                        report.by_format.entry(format.as_str()).or_default().failed += 1;
                        write_outcome(
                            store,
                            report,
                            file,
                            extractor,
                            Status::Failed,
                            None,
                            None,
                            0,
                            0,
                            at,
                            tp,
                        )?;
                        continue;
                    }
                }
            }
            Err(detail) => {
                let status = if detail == "binario" {
                    report.binary += 1;
                    Status::Binary
                } else {
                    report.failed += 1;
                    report
                        .errors
                        .push(format!("{}: {detail}", file.file_name()));
                    report.by_format.entry(format.as_str()).or_default().failed += 1;
                    Status::Failed
                };
                write_outcome(
                    store,
                    report,
                    file,
                    format.as_str(),
                    status,
                    None,
                    None,
                    0,
                    0,
                    at,
                    tp,
                )?;
                continue;
            }
        };

        // ── 6 · normalizar ANTES de medir: los offsets son de este texto ──
        let mut normalized = text::normalize(&raw);

        // Un PDF sin texto es, casi siempre, un escaneo: con OCR se lee.
        if normalized.is_empty() && format == Format::Pdf && extractor == "pdf" {
            if let Some(o) = ocr {
                extractor = "pdf+ocr";
                match o.pdf(&bytes, obrain_ocr::MAX_PDF_PAGES) {
                    Some(Ok(t)) => {
                        normalized = text::normalize(&t);
                        leido_con = Some(o.engine());
                    }
                    Some(Err(e)) => report.errors.push(format!("{}: {e}", file.file_name())),
                    None => {}
                }
            }
        }

        if normalized.is_empty() {
            report.empty += 1;
            report.by_format.entry(format.as_str()).or_default().empty += 1;
            write_outcome(
                store,
                report,
                file,
                extractor,
                Status::Empty,
                None,
                None,
                0,
                0,
                at,
                tp,
            )?;
            continue;
        }

        // ── 7 · ¿es texto, pero no es conocimiento? ──
        // Va aquí y no antes porque la señal está en el texto, no en la ruta:
        // es la única forma de distinguir un bundle minificado de un `.js`
        // escrito a mano sin mantener una lista de nombres para siempre.
        if let Some(kind) = noise::classify(&normalized) {
            report.noise += 1;
            report.by_format.entry(format.as_str()).or_default().noise += 1;
            let status = match kind {
                noise::Noise::Minified => Status::Minified,
                noise::Noise::WordList => Status::WordList,
            };
            write_outcome(
                store, report, file, extractor, status, None, None, 0, 0, at, tp,
            )?;
            continue;
        }

        let hash = text::text_hash(&normalized);
        let chunks = chunk::chunk(&normalized);
        index::replace_chunks(store.conn(), &file.entity, chunker, chunker_ver, &chunks)?;

        report.extracted += 1;
        if leido_con.is_some() {
            report.ocr += 1;
        }
        report.chunks += chunks.len();
        report.chars += normalized.chars().count() as u64;
        let stat = report.by_format.entry(format.as_str()).or_default();
        stat.chunks += chunks.len();
        stat.chars += normalized.chars().count() as u64;

        write_outcome(
            store,
            report,
            file,
            extractor,
            Status::Ok,
            leido_con.as_deref(),
            Some(&hash),
            normalized.chars().count(),
            chunks.len(),
            at,
            tp,
        )?;
    }
    Ok(())
}

/// Escribe la fila de la proyección y, si aporta algo, las aserciones.
#[allow(clippy::too_many_arguments)]
fn write_outcome(
    store: &Store,
    report: &mut ExtractReport,
    file: &FileRow,
    extractor: &str,
    status: Status,
    ocr_engine: Option<&str>,
    text_hash: Option<&str>,
    chars: usize,
    chunk_count: usize,
    at: i64,
    tp: TimePoint,
) -> Result<()> {
    index::record(
        store.conn(),
        index::Record {
            entity: &file.entity,
            extractor,
            // Hoy todos los extractores son la versión 1 (`Format::version`).
            extractor_ver: 1,
            chunker: chunk::CHUNKER,
            chunker_ver: chunk::CHUNKER_VERSION,
            status: status.as_str(),
            file_hash: file.file_hash.as_deref(),
            text_hash,
            chars,
            chunk_count,
            at,
            detail: None,
        },
    )?;

    // Si este archivo tenía fragmentos y ahora no produce texto indexable
    // —lo editaron, se corrompió, resultó ser un bundle—, se van. Una
    // proyección que conserva el pasado no es una proyección: es un log mal
    // hecho, y ya hay uno bien hecho al lado.
    if status != Status::Ok {
        index::replace_chunks(
            store.conn(),
            &file.entity,
            chunk::CHUNKER,
            chunk::CHUNKER_VERSION,
            &[],
        )?;
    }

    if !status.worth_asserting() {
        return Ok(());
    }

    assert_if_changed(
        store,
        report,
        file,
        "extraction_status",
        status.as_str().into(),
        tp,
        at,
    )?;
    if status == Status::Ok {
        assert_if_changed(
            store,
            report,
            file,
            "text_chars",
            Value::Number(chars as f64),
            tp,
            at,
        )?;
        assert_if_changed(
            store,
            report,
            file,
            "chunk_count",
            Value::Number(chunk_count as f64),
            tp,
            at,
        )?;
        if let Some(h) = text_hash {
            assert_if_changed(store, report, file, "text_hash", h.into(), tp, at)?;
        }
        // I7: el chunker queda en el log, no sólo en la tabla. Cuando M4
        // escriba vectores, esta aserción es la que dice de qué fragmentos son.
        assert_if_changed(
            store,
            report,
            file,
            "chunker",
            chunk::chunker_id().into(),
            tp,
            at,
        )?;
        // Quién leyó el texto, si no estaba en el archivo. Y si ahora sí lo
        // estaba —el PDF se rehízo con texto—, lo del OCR deja de ser cierto.
        match ocr_engine {
            Some(e) => assert_if_changed(store, report, file, "ocr_engine", e.into(), tp, at)?,
            None => {
                for a in store.query(Some(&file.entity), Some("ocr_engine"), tp)? {
                    store.retract(&a.id, at)?;
                }
            }
        }
        // La faceta es lo que convierte a la entidad en algo con contenido
        // buscable, y por tanto en candidata a embeddings en M4.
        if !store
            .facets(&file.entity, tp)?
            .iter()
            .any(|f| f == Facet::Textual.as_str())
        {
            store.assert(
                NewAssertion::new(
                    file.entity.clone(),
                    "has_facet",
                    Facet::Textual.as_str(),
                    Method::Deterministic,
                    at,
                )
                .source("extract@m2"),
            )?;
            report.assertions_written += 1;
        }
    }
    Ok(())
}

/// Misma disciplina que `obrain-scan`: reafirmar lo idéntico sólo infla el log.
fn assert_if_changed(
    store: &Store,
    report: &mut ExtractReport,
    file: &FileRow,
    predicate: &str,
    value: Value,
    tp: TimePoint,
    at: i64,
) -> Result<()> {
    if let Some(current) = store.resolve(&file.entity, predicate, tp)? {
        if current.object == value {
            report.assertions_unchanged += 1;
            return Ok(());
        }
    }
    store.assert(
        NewAssertion::new(
            file.entity.clone(),
            predicate,
            value,
            Method::Deterministic,
            at,
        )
        .source("extract@m2"),
    )?;
    report.assertions_written += 1;
    Ok(())
}
