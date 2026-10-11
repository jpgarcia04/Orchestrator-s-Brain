//! El apóstrofo de `Orchestrator's-Brain`, que el maestro §10 dejó como
//! decisión pendiente, se cobra su primera pieza aquí.
//!
//! `tauri-winres` genera un `resource.rc` con la ruta absoluta del icono y la
//! pasa por `escape_string`, que convierte `'` en `\'`. `RC.EXE` sólo entiende
//! `\\` y `\"`, así que se lleva el escape tal cual y busca una carpeta
//! literalmente llamada `Orchestrator\'s-Brain`:
//!
//! ```text
//! resource.rc(26) : error RC2135 : file not found:
//!   C:\...\GitHub\Orchestrator\'s-Brain\ui\src-tauri\icons\icon.ico
//! ```
//!
//! No se arregla con un enlace de directorio: `tauri-build` canonicaliza la
//! ruta y vuelve a salir la real. La solución de verdad es que la carpeta no
//! lleve apóstrofo — es la decisión pendiente, y no la toma un `build.rs`.
//!
//! Mientras tanto, el icono se copia a una ruta temporal sin comillas y se le
//! dice a winres que mire allí. **El parche está condicionado a que haya
//! apóstrofo**, así que el día que la carpeta se renombre este archivo vuelve a
//! ser la línea de siempre sin que nadie tenga que acordarse de quitarlo.

use std::path::PathBuf;

fn main() {
    let icono = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("icons")
        .join("icon.ico");

    let mut atributos = tauri_build::Attributes::new();

    if icono.to_string_lossy().contains('\'') {
        let destino = std::env::temp_dir().join("obrain-winres");
        std::fs::create_dir_all(&destino).expect("no se pudo crear el directorio del icono");
        let copia = destino.join("icon.ico");
        std::fs::copy(&icono, &copia).expect("no se pudo copiar el icono");
        assert!(
            !copia.to_string_lossy().contains('\''),
            "tambien hay un apostrofo en el directorio temporal: {}",
            copia.display()
        );
        println!("cargo:rerun-if-changed={}", icono.display());
        atributos = atributos
            .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(&copia));
    }

    tauri_build::try_build(atributos).expect("no se pudo preparar la compilacion de Tauri");
}
