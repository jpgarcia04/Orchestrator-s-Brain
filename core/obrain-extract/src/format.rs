//! Qué sabemos leer, y cómo se decide **antes de abrir el archivo**.
//!
//! La detección es por extensión a propósito. Olfatear el contenido exige
//! leer bytes, y leer bytes es justo lo que el triaje de M1 nos prohíbe hacer
//! antes de saber si el archivo es local (`PROJECT-MASTER.md` §4.3). El orden
//! correcto es: extensión → disponibilidad → abrir. Nunca al revés.
//!
//! El olfateo de contenido sigue existiendo, pero **después** de abrir y sólo
//! para descartar (ver [`crate::text::looks_binary`]): un `.log` de LevelDB
//! tiene la misma extensión que un cuaderno de notas.

pub mod html;
pub mod ooxml;
pub mod pdf;
pub mod plaintext;

/// Un formato del que sabemos sacar texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Format {
    /// Texto plano, Markdown, configuración y **código fuente**. La faceta
    /// `Textual` del maestro (§3.2) incluye el código explícitamente.
    PlainText,
    Html,
    Pdf,
    /// `docx`, `xlsx`, `pptx` — ZIP con XML dentro.
    Ooxml,
    /// Una foto o un escaneo. **Sólo se lee con OCR** (`obrain-ocr`): sin
    /// reconocedor, se salta igual que un formato desconocido.
    Image,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::PlainText => "plaintext",
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Ooxml => "ooxml",
            Format::Image => "image",
        }
    }

    /// Versión del extractor. Cambiarla obliga a reextraer: es lo que hace
    /// reconstruible la proyección (I1).
    pub fn version(self) -> i64 {
        1
    }

    /// Extrae texto de los bytes del archivo.
    pub fn extract(self, bytes: &[u8]) -> Result<String, String> {
        match self {
            Format::PlainText => plaintext::extract(bytes),
            Format::Html => html::extract(bytes),
            Format::Pdf => pdf::extract(bytes),
            Format::Ooxml => ooxml::extract(bytes),
            Format::Image => Err("una imagen sólo se lee con OCR".to_string()),
        }
    }
}

/// Extensiones que tratamos como texto plano.
///
/// La lista es larga y no pide perdón: en un disco real de alguien que
/// programa, el código **es** documentación de lo que estuvo haciendo, y el
/// coste de leer un `.ts` es idéntico al de leer un `.txt`.
const PLAIN: &[&str] = &[
    // prosa y notas
    "txt",
    "md",
    "markdown",
    "rst",
    "adoc",
    "tex",
    "csv",
    "tsv",
    "srt",
    "vtt",
    "log",
    // configuración
    "ini",
    "cfg",
    "conf",
    "env",
    "yml",
    "yaml",
    "toml",
    "json",
    "jsonl",
    "xml",
    "properties",
    "gitignore",
    "gitattributes",
    "editorconfig",
    "example",
    "lock",
    // código
    "rs",
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "tsx",
    "vue",
    "svelte",
    "py",
    "rb",
    "php",
    "go",
    "java",
    "kt",
    "swift",
    "c",
    "h",
    "cpp",
    "hpp",
    "cc",
    "cs",
    "sql",
    "sh",
    "bash",
    "zsh",
    "ps1",
    "psm1",
    "psd1",
    "ps1xml",
    "bat",
    "cmd",
    "css",
    "scss",
    "sass",
    "less",
    "prisma",
    "graphql",
    "proto",
    "r",
    "lua",
    "pl",
    "dart",
    "ex",
    "exs",
];

/// Archivos sin extensión que sí son texto y que aparecen de verdad en repos.
/// Lista corta y cerrada: el resto de los archivos sin extensión de este corpus
/// son partidas guardadas y datos de aplicación (ver el informe de M2).
const PLAIN_NAMES: &[&str] = &[
    "dockerfile",
    "caddyfile",
    "makefile",
    "license",
    "licence",
    "readme",
    "changelog",
    "notice",
    "authors",
    "procfile",
];

/// Formato de un archivo, o `None` si no sabemos leerlo.
///
/// Función pura sobre la ruta: **no toca el disco**.
pub fn detect(path: &str) -> Option<Format> {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    // Sin punto no hay extensión, y punto.
    //
    // Encontrado auditando el corpus real: la versión anterior usaba el nombre
    // entero como extensión cuando no había punto, para que `.gitignore`
    // funcionara. El efecto colateral fue que los archivos `LOG` y `LOCK` de
    // LevelDB —que no tienen extensión, se llaman así— caían en las
    // extensiones `log` y `lock` y se abrían. Diez archivos abiertos por una
    // conveniencia sintáctica.
    let ext = match name.rsplit_once('.') {
        Some((_, e)) => e.to_ascii_lowercase(),
        None => String::new(),
    };

    match ext.as_str() {
        "pdf" => Some(Format::Pdf),
        "html" | "htm" | "xhtml" => Some(Format::Html),
        "docx" | "docm" | "xlsx" | "xlsm" | "pptx" | "pptm" => Some(Format::Ooxml),
        // Lo que decodifica Windows sin instalar nada más; `heic` y `webp` si
        // tiene sus extensiones, y si no, el reconocedor lo dice y se anota.
        "jpg" | "jpeg" | "png" | "bmp" | "gif" | "tif" | "tiff" | "webp" | "heic" => {
            Some(Format::Image)
        }
        e if PLAIN.contains(&e) => Some(Format::PlainText),
        _ => {
            let lower = name.to_ascii_lowercase();
            PLAIN_NAMES
                .contains(&lower.as_str())
                .then_some(Format::PlainText)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconoce_los_formatos_del_corpus_real() {
        assert_eq!(detect(r"C:\a\certificado-matricula.pdf"), Some(Format::Pdf));
        assert_eq!(detect(r"C:\a\index.html"), Some(Format::Html));
        assert_eq!(detect(r"C:\a\Estimado Sr.docx"), Some(Format::Ooxml));
        assert_eq!(detect(r"C:\a\PROJECT-MASTER.md"), Some(Format::PlainText));
        assert_eq!(detect(r"C:\a\Dockerfile"), Some(Format::PlainText));
        assert_eq!(detect(r"C:\a\.gitignore"), Some(Format::PlainText));
    }

    #[test]
    fn no_intenta_leer_lo_que_no_es_texto() {
        for p in [
            r"C:\a\video.mp4",
            r"C:\a\PROFSAVE_backup",
            r"C:\Rockstar Games\GTA V\Profiles\40BCFAA0\SGTA50015",
            r"C:\a\preloaded_data.pb",
            r"C:\a\biblioteca.dll",
        ] {
            assert_eq!(detect(p), None, "{p} no deberia intentarse");
        }
    }

    /// Regresión del corpus real: `LOG` y `LOCK` son archivos de LevelDB **sin
    /// extensión**, no archivos con extensión `.log` y `.lock`.
    #[test]
    fn un_nombre_sin_punto_no_tiene_extension() {
        for p in [
            r"C:\a\Default\Sync Data\LevelDB\LOG",
            r"C:\a\Default\Sync Data\LevelDB\LOCK",
            r"C:\a\CURRENT",
            r"C:\a\MANIFEST-000001",
        ] {
            assert_eq!(detect(p), None, "{p} no tiene extension");
        }
        // …pero un `.log` de verdad sí, y los dotfiles siguen funcionando.
        assert_eq!(detect(r"C:\a\launcher.log"), Some(Format::PlainText));
        assert_eq!(detect(r"C:\a\.editorconfig"), Some(Format::PlainText));
    }

    /// Una foto es un formato, pero de los que sólo se leen con OCR: el
    /// pipeline decide qué hacer si no hay reconocedor.
    #[test]
    fn una_foto_es_una_imagen() {
        assert_eq!(
            detect(r"C:\Bóveda\01 Identidad\IMG_2291.JPG"),
            Some(Format::Image)
        );
        assert_eq!(detect(r"C:\a\escaneo.tiff"), Some(Format::Image));
        assert!(
            Format::Image.extract(b"\x89PNG").is_err(),
            "sin OCR no se extrae"
        );
    }

    #[test]
    fn la_extension_no_distingue_mayusculas() {
        assert_eq!(detect(r"C:\a\INFORME.PDF"), Some(Format::Pdf));
        assert_eq!(detect(r"C:\a\Notas.TXT"), Some(Format::PlainText));
    }

    /// Un directorio con punto en el nombre no convierte a su hijo en PDF.
    #[test]
    fn solo_mira_el_nombre_del_archivo() {
        assert_eq!(detect(r"C:\carpeta.pdf\PROFSAVE"), None);
    }
}
