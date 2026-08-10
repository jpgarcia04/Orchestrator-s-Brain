//! Exclusiones. Un escaneo ingenuo de una máquina de desarrollo indexa
//! 400.000 archivos basura antes de llegar a un solo documento útil
//! (`PROJECT-MASTER.md` §4.1).
//!
//! Criterio: se excluye lo que es **derivado, regenerable o del sistema**.
//! Nunca se excluye algo que pueda ser contenido del usuario. Ante la duda,
//! se indexa: un falso negativo es contenido perdido, y eso es peor que ruido.

use std::path::{Path, PathBuf};

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
///
/// Esta lista vale para los que aparecen **a cualquier profundidad** —un
/// `.git` o un `.vscode` dentro de un proyecto—. Los que cuelgan del perfil
/// del usuario los coge una señal, no esta lista: ver
/// [`ExclusionSet::profile_root`].
///
/// Los directorios de credenciales (`.ssh`, `.gnupg`, `.aws`, `.azure`,
/// `.kube`, `.docker`) están aquí y no en la señal **a propósito**: no se
/// indexan estén donde estén. Una clave privada no es conocimiento del
/// usuario, y la puerta de sensibilidad (M3½) es la última línea, no la
/// primera.
const TOOL_DIRS: &[&str] = &[
    ".git", ".hg", ".svn", ".cache", ".pytest_cache", ".mypy_cache",
    ".ruff_cache", ".tox", ".idea", ".vs", ".vscode", ".terraform",
    ".cargo", ".rustup", ".nvm", ".npm", ".pnpm-store", ".yarn", ".m2",
    ".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker",
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
    /// Raíz del perfil del usuario (`%USERPROFILE%`), si se conoce.
    ///
    /// Sostiene la regla de [`ExclusionSet::dotdir_del_perfil`]. Es un campo
    /// y no una constante para que los tests no dependan del entorno.
    pub profile_root: Option<PathBuf>,
}

impl Default for ExclusionSet {
    fn default() -> Self {
        ExclusionSet {
            skip_hidden: true,
            extra_dirs: Vec::new(),
            profile_root: std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from),
        }
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
        if self.dotdir_del_perfil(path, name) {
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

    /// Un directorio con punto que cuelga **directamente** del perfil del
    /// usuario es estado de una herramienta, no contenido.
    ///
    /// Ésta es la señal que la lista fija no podía dar. Un escaneo en seco del
    /// perfil real (2026-08-10) devolvió 25.665 archivos, y **~22.000 de ellos
    /// —el 86 %— eran `.codex`, `.vscode`, `.gemini`, `.antigravity` y `.m2`**.
    /// `TOOL_DIRS` ya tenía `.cargo` y `.rustup`; simplemente se escribió antes
    /// de que existieran los otros. Añadirlos a la lista habría arreglado hoy y
    /// vuelto a fallar con la herramienta del mes que viene — que es
    /// exactamente el techo que M1 dejó anotado.
    ///
    /// La convención en la que se apoya es sólida y anterior a este proyecto:
    /// el punto inicial significa «esto no es tuyo, es de un programa», y en la
    /// raíz del perfil no hay más que eso. Nadie guarda sus fotos en
    /// `C:\Users\x\.vacaciones`.
    ///
    /// **Se limita a la raíz a propósito.** Más adentro, una carpeta con punto
    /// sí puede ser del usuario, y el criterio de este módulo es que ante la
    /// duda se indexa: un falso negativo es contenido perdido.
    fn dotdir_del_perfil(&self, path: &Path, name: &str) -> bool {
        if !name.starts_with('.') {
            return false;
        }
        match (&self.profile_root, path.parent()) {
            (Some(raiz), Some(padre)) => misma_ruta(padre, raiz),
            _ => false,
        }
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

/// Igualdad de rutas como la entiende Windows: sin distinguir mayúsculas y
/// sin que importe si el separador vino como `\` o como `/`.
///
/// `PathBuf::eq` compara componente a componente —lo cual ya resuelve el
/// separador— pero **distingue mayúsculas**, y aquí un lado sale de
/// `%USERPROFILE%` y el otro de la ruta que escribió quien lanzó el escaneo.
/// `C:\Users\pacox` y `C:\USERS\pacox` son el mismo sitio.
fn misma_ruta(a: &Path, b: &Path) -> bool {
    let (mut ca, mut cb) = (a.components(), b.components());
    loop {
        match (ca.next(), cb.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) => {
                if !x
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&y.as_os_str().to_string_lossy())
                {
                    return false;
                }
            }
            _ => return false,
        }
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

    /// Perfil fijo: los tests de la señal no pueden depender de en qué máquina
    /// corren.
    fn con_perfil(raiz: &str) -> ExclusionSet {
        ExclusionSet { profile_root: Some(PathBuf::from(raiz)), ..Default::default() }
    }

    /// El hallazgo del 2026-08-10. Un escaneo en seco del perfil real devolvió
    /// 25.665 archivos y ~22.000 eran esto: estado de herramientas de IA y de
    /// desarrollo que `TOOL_DIRS` no cubría porque se escribió antes de que
    /// existieran.
    ///
    /// El caso que justifica la señal es el último: **una herramienta que aún
    /// no existe**. Con una lista fija, ese assert es imposible.
    #[test]
    fn poda_el_estado_de_herramientas_que_cuelga_del_perfil() {
        let e = con_perfil("C:/Users/x");
        for name in [".codex", ".gemini", ".antigravity", ".claude", ".m2", ".loquesalgaelmesqueviene"] {
            assert_eq!(
                e.skip_dir(&d(&format!("C:/Users/x/{name}")), attr::DIRECTORY),
                Some(SkipReason::ToolInternal),
                "{name} cuelga del perfil y empieza por punto"
            );
        }
    }

    /// La otra mitad de la regla, y el motivo de que esté acotada a la raíz:
    /// más adentro, una carpeta con punto puede ser del usuario. Perderla sería
    /// el error caro de este módulo.
    #[test]
    fn una_carpeta_con_punto_mas_adentro_es_del_usuario() {
        let e = con_perfil("C:/Users/x");
        assert_eq!(e.skip_dir(&d("C:/Users/x/Documentos/.borradores"), attr::DIRECTORY), None);
        assert_eq!(e.skip_dir(&d("C:/Users/x/OneDrive/.notas de la visa"), attr::DIRECTORY), None);
    }

    /// Y la señal no puede llevarse por delante el perfil entero.
    #[test]
    fn la_regla_del_perfil_no_toca_las_carpetas_normales() {
        let e = con_perfil("C:/Users/x");
        for name in ["Documentos", "Downloads", "OneDrive", "Imágenes", "Escritorio"] {
            assert_eq!(
                e.skip_dir(&d(&format!("C:/Users/x/{name}")), attr::DIRECTORY),
                None,
                "{name} es donde vive lo que hay que indexar"
            );
        }
    }

    /// Las credenciales están en la lista y no en la señal: no se indexan
    /// estén donde estén. La puerta de sensibilidad es la última línea, no la
    /// primera.
    #[test]
    fn las_credenciales_se_podan_a_cualquier_profundidad() {
        let e = con_perfil("C:/Users/x");
        for name in [".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker"] {
            assert_eq!(
                e.skip_dir(&d(&format!("C:/Users/x/Proyectos/servidor/{name}")), attr::DIRECTORY),
                Some(SkipReason::ToolInternal),
                "{name} no es conocimiento del usuario"
            );
        }
    }

    /// `%USERPROFILE%` y la ruta que escribe una persona no tienen por qué
    /// coincidir ni en mayúsculas ni en separador.
    #[test]
    fn la_raiz_del_perfil_se_compara_como_lo_hace_windows() {
        let e = con_perfil("C:\\Users\\Pacox");
        assert_eq!(
            e.skip_dir(&d("c:/users/pacox/.codex"), attr::DIRECTORY),
            Some(SkipReason::ToolInternal)
        );
    }

    /// Sin perfil conocido, la señal se apaga entera en vez de adivinar.
    #[test]
    fn sin_perfil_la_senal_no_poda_nada() {
        let e = ExclusionSet { profile_root: None, ..Default::default() };
        assert_eq!(e.skip_dir(&d("C:/Users/x/.codex"), attr::DIRECTORY), None);
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
