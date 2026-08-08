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
    /// Paquetes de terceros instalados por un gestor. El `node_modules` de
    /// otra cosa: se reinstalan con un comando y nadie los escribió aquí.
    Dependency,
    /// Directorio de perfil de un navegador incrustado (Chromium/CEF).
    /// Se detecta por marcador, no por nombre.
    BrowserProfile,
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
            SkipReason::Dependency => "dependencia de terceros",
            SkipReason::BrowserProfile => "perfil de navegador incrustado",
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

/// Directorios de paquetes de terceros que **sólo** son basura cuando cuelgan
/// de un padre concreto. Se comparan como par `(padre, hijo)`.
///
/// El informe de M2 cuantificó que `WindowsPowerShell\Modules` y
/// `PowerShell\Modules` aportaban **1.030 fragmentos, el 20,5 % del índice
/// léxico** — la ayuda XML de `Renci.SshNet`, de `Posh-SSH` y compañía. Son el
/// `node_modules` de PowerShell: los instaló `Install-Module`, no una persona.
///
/// El par importa. `Modules` a secas es una palabra que cualquiera puede usar
/// para una carpeta suya, y podarla por nombre sería el error caro de este
/// módulo. `WindowsPowerShell\Scripts`, en cambio, **no** se poda: ahí es donde
/// la gente guarda sus propios scripts.
const DEPENDENCY_PAIRS: &[(&str, &str)] = &[
    ("WindowsPowerShell", "Modules"),
    ("PowerShell", "Modules"),
];

/// Marcadores de un directorio de datos de usuario de Chromium.
///
/// El otro 34,6 % del índice que midió M2 salía de
/// `Rockstar Games\Social Club\Launcher\Renderer`: un navegador incrustado
/// (CEF) que vuelca su perfil entero dentro de `Documentos`. Las cachés ya se
/// podaban por nombre, pero las cargas del *component updater*
/// —`AutofillStates`, `FileTypePolicies`, `CertificateRevocation`,
/// `open_cookie_database.json`— no, y son texto en inglés bien formado que
/// ninguna señal de contenido distingue de un documento legítimo.
///
/// M1 dejó escrito que la lista fija tiene techo y que hacía falta una señal.
/// Ésta lo es: un perfil de Chromium **se identifica a sí mismo**. `Local
/// State` es su archivo de preferencias globales y `First Run` la marca que
/// escribe al arrancar por primera vez. Exigir los dos evita confundirlo con
/// una carpeta que casualmente se llame igual.
const CHROMIUM_MARKERS: &[&str] = &["Local State", "First Run"];

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

    /// ¿Hay que podar este directorio? Se decide **antes de descender**, que
    /// es lo que hace la poda barata: los archivos de dentro no se enumeran.
    ///
    /// Tres clases de señal, de más barata a más cara:
    ///
    /// 1. **El nombre** — `node_modules`, `.git`, `GPUCache`.
    /// 2. **El par padre/hijo** — `WindowsPowerShell\Modules`. Existe porque
    ///    `Modules` a secas es una palabra legítima del usuario.
    /// 3. **Un marcador dentro** — el perfil de Chromium. Cuesta dos
    ///    `symlink_metadata`, y sólo se paga cuando las dos primeras fallan.
    pub fn skip_dir(&self, path: &Path, attributes: u32) -> Option<SkipReason> {
        if crate::triage::is_plain_reparse(attributes) {
            return Some(SkipReason::Reparse);
        }
        let name = path.file_name()?.to_string_lossy();
        let name = name.as_ref();

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
        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned());
        if let Some(parent) = &parent {
            if DEPENDENCY_PAIRS
                .iter()
                .any(|(p, c)| p.eq_ignore_ascii_case(parent) && c.eq_ignore_ascii_case(name))
            {
                return Some(SkipReason::Dependency);
            }
        }
        if is_browser_profile(path) {
            return Some(SkipReason::BrowserProfile);
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

/// ¿Es este directorio el perfil de un Chromium incrustado?
///
/// Se pregunta al directorio, no a una lista. Un `Local State` junto a un
/// `First Run` sólo los escribe Chromium, y no hay motivo por el que una
/// persona tuviera los dos con esos nombres exactos en la misma carpeta.
///
/// `symlink_metadata` en vez de `exists`: pregunta por atributos y no abre
/// nada, así que un marcador que fuera un placeholder de OneDrive tampoco
/// dispararía la hidratación. Esta función corre dentro del recorrido y el
/// criterio de M1 —cero descargas— no admite excepciones por comodidad.
fn is_browser_profile(dir: &Path) -> bool {
    CHROMIUM_MARKERS
        .iter()
        .all(|m| std::fs::symlink_metadata(dir.join(m)).is_ok_and(|m| m.is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triage::attr;
    use std::path::PathBuf;

    /// Los tests de nombre no tocan el disco: `skip_dir` sólo mira el sistema
    /// de archivos cuando las reglas de nombre y de par ya han fallado.
    fn d(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    #[test]
    fn poda_lo_regenerable() {
        let e = ExclusionSet::new();
        assert_eq!(e.skip_dir(&d("x/node_modules"), attr::DIRECTORY), Some(SkipReason::BuildArtifact));
        assert_eq!(e.skip_dir(&d("x/target"), attr::DIRECTORY), Some(SkipReason::BuildArtifact));
        assert_eq!(e.skip_dir(&d("x/.git"), attr::DIRECTORY), Some(SkipReason::ToolInternal));
        assert_eq!(e.skip_dir(&d("x/AppData"), attr::DIRECTORY), Some(SkipReason::SystemPath));
    }

    #[test]
    fn poda_las_caches_que_las_apps_dejan_en_documentos() {
        let e = ExclusionSet::new();
        for name in ["Cache", "Code Cache", "GPUCache", "Service Worker", "IndexedDB"] {
            assert_eq!(
                e.skip_dir(&d(&format!("x/{name}")), attr::DIRECTORY),
                Some(SkipReason::AppData),
                "{name} es caché de aplicación, no conocimiento"
            );
        }
    }

    #[test]
    fn no_poda_contenido_del_usuario() {
        let e = ExclusionSet::new();
        for name in ["Documentos", "Fotos", "Proyectos", "visa", "facturas 2023"] {
            assert_eq!(e.skip_dir(&d(&format!("x/{name}")), attr::DIRECTORY), None, "{name} es del usuario");
        }
    }

    /// El 20,5 % del índice léxico de M2 salía de aquí.
    #[test]
    fn poda_los_modulos_de_powershell() {
        let e = ExclusionSet::new();
        for parent in ["WindowsPowerShell", "PowerShell"] {
            assert_eq!(
                e.skip_dir(&d(&format!("C:/Users/x/Documentos/{parent}/Modules")), attr::DIRECTORY),
                Some(SkipReason::Dependency),
                "{parent}\\Modules es el node_modules de PowerShell"
            );
        }
    }

    /// La otra mitad de la regla, y la que justifica que sea un par y no un
    /// nombre: `Modules` es una palabra normal y `Scripts` es donde la gente
    /// guarda lo suyo.
    #[test]
    fn modules_a_secas_no_se_poda() {
        let e = ExclusionSet::new();
        assert_eq!(e.skip_dir(&d("C:/Users/x/Documentos/Curso Rust/Modules"), attr::DIRECTORY), None);
        assert_eq!(e.skip_dir(&d("C:/Users/x/Documentos/WindowsPowerShell/Scripts"), attr::DIRECTORY), None);
    }

    /// El 34,6 % restante. Se detecta por marcador porque ninguna de las
    /// palabras de la ruta —`Rockstar Games`, `Social Club`, `Launcher`,
    /// `Renderer`— dice "esto es un navegador".
    #[test]
    fn poda_un_perfil_de_chromium_por_su_marcador() {
        let dir = std::env::temp_dir().join("obrain-test-perfil-chromium");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let e = ExclusionSet::new();
        assert_eq!(e.skip_dir(&dir, attr::DIRECTORY), None, "todavía no hay marcadores");

        std::fs::write(dir.join("Local State"), b"{}").unwrap();
        assert_eq!(e.skip_dir(&dir, attr::DIRECTORY), None, "un solo marcador no basta");

        std::fs::write(dir.join("First Run"), b"").unwrap();
        assert_eq!(e.skip_dir(&dir, attr::DIRECTORY), Some(SkipReason::BrowserProfile));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_desciende_por_junctions() {
        let e = ExclusionSet::new();
        assert_eq!(
            e.skip_dir(&d("x/cualquiera"), attr::DIRECTORY | attr::REPARSE_POINT),
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
                &d("x/Documentos"),
                attr::DIRECTORY | attr::REPARSE_POINT | attr::RECALL_ON_DATA_ACCESS
            ),
            None
        );
    }
}
