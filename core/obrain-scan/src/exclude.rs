//! Exclusiones. Un escaneo ingenuo de una máquina de desarrollo indexa
//! 400.000 archivos basura antes de llegar a un solo documento útil
//! (`PROJECT-MASTER.md` §4.1).
//!
//! Criterio: se excluye lo que es **derivado, regenerable o del sistema**.
//! Nunca se excluye algo que pueda ser contenido del usuario. Ante la duda,
//! se indexa: un falso negativo es contenido perdido, y eso es peor que ruido.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Directorio de artefactos regenerables.
    BuildArtifact,
    /// Interno de una herramienta (git, cachés).
    ToolInternal,
    /// Caché o estado de una aplicación: no es conocimiento del usuario.
    AppData,
    /// Del sistema operativo.
    SystemPath,
    /// Junction o symlink: recorrerlo invita a ciclos.
    Reparse,
    /// Archivo oculto o de sistema.
    HiddenOrSystem,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::BuildArtifact => "artefacto de compilacion",
            SkipReason::ToolInternal => "interno de herramienta",
            SkipReason::AppData => "cache de aplicacion",
            SkipReason::SystemPath => "ruta del sistema",
            SkipReason::Reparse => "reparse point",
            SkipReason::HiddenOrSystem => "oculto o de sistema",
        }
    }
}

/// Regenerables: si se borran, una herramienta los reconstruye. No son
/// conocimiento.
const BUILD_DIRS: &[&str] = &[
    "node_modules", "target", "__pycache__", ".venv", "venv", "env",
    ".next", ".nuxt", ".svelte-kit", ".parcel-cache", ".turbo",
    "bower_components", "vendor", "Pods", ".gradle", ".mvn",
    "obj", "bin", "Debug", "Release", "CMakeFiles",
];

/// Internos de herramientas. `.git` se excluye **como directorio**: el
/// historial es valioso, pero se leerá con una librería de git en M3, no
/// indexando objetos comprimidos uno a uno.
const TOOL_DIRS: &[&str] = &[
    ".git", ".hg", ".svn", ".cache", ".pytest_cache", ".mypy_cache",
    ".ruff_cache", ".tox", ".idea", ".vs", ".terraform",
    ".cargo", ".rustup", ".nvm", ".npm", ".pnpm-store", ".yarn",
];

/// Datos de aplicación: cachés de navegador, perfiles, almacenes locales.
///
/// Descubierto al auditar el grafo de M3 con `--sample`: la mitad de las
/// aristas del primer escaneo real enlazaban `f_000024`, `data_1`, `index` y
/// `hyph-ka.hyb` — cachés de Chromium que las aplicaciones vuelcan **dentro de
/// `Documentos`**. Ningún filtro por extensión los habría cogido; el patrón
/// está en el directorio.
///
/// Es la demostración de por qué M3 tenía que poder auditarse: los contadores
/// decían "4.336 aristas" y parecían un éxito.
const APP_DATA_DIRS: &[&str] = &[
    "Cache", "Cache_Data", "Code Cache", "GPUCache", "DawnCache", "DawnGraphiteCache",
    "ShaderCache", "GrShaderCache", "CacheStorage", "Service Worker", "blob_storage",
    "IndexedDB", "Local Storage", "Session Storage", "Crashpad", "component_crx_cache",
    "extensions_crx_cache", "Safe Browsing", "WidevineCdm", "hyphen-data",
    "Local State", "Network", "Sessions", "Storage", "logs", "Logs", "CrashReports",
];

/// Rutas del sistema operativo. Se comparan como componente de ruta.
const SYSTEM_DIRS: &[&str] = &[
    "AppData", "Application Data", "$Recycle.Bin", "System Volume Information",
    "Windows", "Program Files", "Program Files (x86)", "ProgramData",
    "Recovery", "PerfLogs", "MSOCache", "$WinREAgent",
];

pub struct ExclusionSet {
    /// Excluir también archivos ocultos y de sistema.
    pub skip_hidden: bool,
    /// Nombres de directorio adicionales indicados por el usuario.
    pub extra_dirs: Vec<String>,
}

impl Default for ExclusionSet {
    fn default() -> Self {
        ExclusionSet { skip_hidden: true, extra_dirs: Vec::new() }
    }
}

impl ExclusionSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// ¿Hay que podar este directorio? Se decide por **nombre**, para no
    /// descender nunca dentro (podar es mucho más barato que filtrar después).
    pub fn skip_dir(&self, name: &str, attributes: u32) -> Option<SkipReason> {
        if crate::triage::is_plain_reparse(attributes) {
            return Some(SkipReason::Reparse);
        }
        if self.extra_dirs.iter().any(|d| d.eq_ignore_ascii_case(name)) {
            return Some(SkipReason::ToolInternal);
        }
        if BUILD_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name)) {
            return Some(SkipReason::BuildArtifact);
        }
        if TOOL_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name)) {
            return Some(SkipReason::ToolInternal);
        }
        if APP_DATA_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name)) {
            return Some(SkipReason::AppData);
        }
        if SYSTEM_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name)) {
            return Some(SkipReason::SystemPath);
        }
        if self.skip_hidden
            && attributes & (crate::triage::attr::SYSTEM | crate::triage::attr::HIDDEN) != 0
        {
            return Some(SkipReason::HiddenOrSystem);
        }
        None
    }

    /// Asimetría deliberada respecto a [`Self::skip_dir`]: aquí se excluye por
    /// `SYSTEM` pero **no** por `HIDDEN`.
    ///
    /// Un *directorio* oculto casi siempre es de una herramienta (`.vscode`,
    /// `.config`) y podarlo ahorra miles de archivos. Un *archivo* oculto
    /// suele ser contenido del usuario que alguna aplicación marcó, y perderlo
    /// sería un falso negativo — el error caro según el criterio de este módulo.
    pub fn skip_file(&self, name: &str, attributes: u32) -> Option<SkipReason> {
        if self.skip_hidden
            && attributes & crate::triage::attr::SYSTEM != 0
        {
            return Some(SkipReason::HiddenOrSystem);
        }
        // Ruido conocido del sistema de archivos, no contenido.
        const NOISE: &[&str] = &["desktop.ini", "Thumbs.db", ".DS_Store", "ehthumbs.db"];
        if NOISE.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return Some(SkipReason::SystemPath);
        }
        None
    }

    /// Comprobación de ruta completa, para la raíz del escaneo.
    pub fn path_is_excluded(&self, path: &Path) -> Option<SkipReason> {
        for c in path.components() {
            let name = c.as_os_str().to_string_lossy();
            if BUILD_DIRS.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                return Some(SkipReason::BuildArtifact);
            }
            if TOOL_DIRS.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                return Some(SkipReason::ToolInternal);
            }
            if SYSTEM_DIRS.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                return Some(SkipReason::SystemPath);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triage::attr;

    #[test]
    fn poda_lo_regenerable() {
        let e = ExclusionSet::new();
        assert_eq!(e.skip_dir("node_modules", attr::DIRECTORY), Some(SkipReason::BuildArtifact));
        assert_eq!(e.skip_dir("target", attr::DIRECTORY), Some(SkipReason::BuildArtifact));
        assert_eq!(e.skip_dir(".git", attr::DIRECTORY), Some(SkipReason::ToolInternal));
        assert_eq!(e.skip_dir("AppData", attr::DIRECTORY), Some(SkipReason::SystemPath));
    }

    #[test]
    fn poda_las_caches_que_las_apps_dejan_en_documentos() {
        let e = ExclusionSet::new();
        for d in ["Cache", "Code Cache", "GPUCache", "Service Worker", "IndexedDB"] {
            assert_eq!(
                e.skip_dir(d, attr::DIRECTORY),
                Some(SkipReason::AppData),
                "{d} es caché de aplicación, no conocimiento"
            );
        }
    }

    #[test]
    fn no_poda_contenido_del_usuario() {
        let e = ExclusionSet::new();
        for d in ["Documentos", "Fotos", "Proyectos", "visa", "facturas 2023"] {
            assert_eq!(e.skip_dir(d, attr::DIRECTORY), None, "{d} es del usuario");
        }
    }

    #[test]
    fn no_desciende_por_junctions() {
        let e = ExclusionSet::new();
        assert_eq!(
            e.skip_dir("cualquiera", attr::DIRECTORY | attr::REPARSE_POINT),
            Some(SkipReason::Reparse)
        );
    }

    #[test]
    fn una_carpeta_de_onedrive_no_se_confunde_con_una_junction() {
        // Las carpetas sincronizadas llevan REPARSE_POINT + recall.
        // Podarlas seria excluir justo lo que mas queremos indexar.
        let e = ExclusionSet::new();
        assert_eq!(
            e.skip_dir(
                "Documentos",
                attr::DIRECTORY | attr::REPARSE_POINT | attr::RECALL_ON_DATA_ACCESS
            ),
            None
        );
    }
}
