//! El único punto del sistema que toca la red, y sólo cuando alguien lo pide.
//!
//! F1 dice **local-first**. Un modelo hay que bajarlo una vez, y eso no
//! contradice nada; lo que sí lo contradice es que se baje solo. Varias
//! librerías cómodas de Rust —`fastembed-rs` es la más conocida— descargan de
//! HuggingFace en la primera ejecución, y entonces «funciona sin conexión» pasa
//! a ser «funciona sin conexión salvo la primera vez, y salvo cuando cambies de
//! modelo, y salvo cuando se limpie la caché». Eso no es una promesa que se
//! pueda mantener.
//!
//! Aquí la descarga es un subcomando: `obrain-embed --descargar-modelo`. Nada
//! más del crate abre un socket, y [`crate::e5::E5::load`] falla con un error
//! claro si los archivos no están en vez de ir a buscarlos.
//!
//! ## Lo que no hace
//!
//! No verifica firmas. Se apoya en TLS y en que el origen sea el repositorio
//! oficial del modelo, que es lo mismo en lo que se apoya cualquier `pip
//! install`, y no es suficiente para un sistema que se toma en serio la cadena
//! de suministro. Comprobar el hash publicado contra el archivo bajado es el
//! paso siguiente y está anotado en el informe, no escondido aquí.

use crate::{Error, Result, e5};
use std::path::{Path, PathBuf};

/// El repositorio oficial del modelo. `intfloat` es el equipo que lo publicó y
/// que publicó el artículo; no una reexportación de terceros.
pub const BASE: &str = "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main";

/// Qué archivo del repositorio va a qué nombre local. `model.onnx` vive en un
/// subdirectorio en el origen y aquí no.
pub const REMOTE: &[(&str, &str)] = &[
    ("onnx/model.onnx", "model.onnx"),
    ("tokenizer.json", "tokenizer.json"),
    ("config.json", "config.json"),
];

/// Dónde viven los modelos si nadie dice otra cosa.
///
/// Fuera del repositorio y fuera de la base de datos: un modelo es un binario
/// grande y reemplazable, compartido entre todas las bases del usuario. La base
/// guarda **qué** modelo produjo cada vector (I7), no el modelo.
pub fn default_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("obrain").join("models").join(e5::MODEL.id)
}

/// ¿Están los tres archivos?
pub fn is_present(dir: &Path) -> bool {
    e5::FILES.iter().all(|f| dir.join(f).is_file())
}

/// Las órdenes exactas para bajarlo a mano.
///
/// Existe porque un paso de red que sólo se puede ejecutar desde dentro del
/// programa es un paso que el usuario no puede auditar. Con esto puede mirar lo
/// que se va a bajar, bajarlo por su cuenta en una máquina con conexión y
/// copiar el directorio a otra que no la tenga.
pub fn manual_instructions(dir: &Path) -> String {
    let mut s = format!("  mkdir \"{}\"\n", dir.display());
    for (remote, local) in REMOTE {
        s.push_str(&format!(
            "  curl -L -o \"{}\" {BASE}/{remote}\n",
            dir.join(local).display()
        ));
    }
    s
}

/// Baja los archivos que falten. Devuelve los bytes escritos.
///
/// Sin dependencia de HTTP en el crate: se delega en `curl`, que Windows 10
/// 1803 y posteriores traen de serie. Añadir un cliente TLS entero al núcleo
/// para tres GET que se ejecutan una vez en la vida del sistema sería pagar
/// una dependencia permanente por una operación excepcional.
pub fn fetch(dir: &Path) -> Result<u64> {
    std::fs::create_dir_all(dir)?;
    let mut total = 0u64;
    for (remote, local) in REMOTE {
        let dest = dir.join(local);
        if dest.is_file() {
            println!("  ya estaba   {local}");
            total += std::fs::metadata(&dest)?.len();
            continue;
        }
        let url = format!("{BASE}/{remote}");
        println!("  bajando     {local}  <- {url}");
        // A un temporal y luego renombrar: una descarga cortada a la mitad no
        // puede dejar un `model.onnx` truncado que la próxima ejecución dé por
        // bueno.
        let tmp = dir.join(format!("{local}.parcial"));
        let status = std::process::Command::new("curl")
            .args(["-L", "--fail", "--silent", "--show-error", "-o"])
            .arg(&tmp)
            .arg(&url)
            .status()
            .map_err(|e| Error::Model(format!("no se pudo ejecutar curl: {e}")))?;
        if !status.success() {
            let _ = std::fs::remove_file(&tmp);
            return Err(Error::Model(format!("curl fallo al bajar {remote}")));
        }
        total += std::fs::metadata(&tmp)?.len();
        std::fs::rename(&tmp, &dest)?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La comprobación no puede dar por bueno un directorio a medias: eso
    /// convertiría un fallo de red en un error de ONNX Runtime tres pasos
    /// después.
    #[test]
    fn un_directorio_incompleto_no_esta_presente() {
        let dir = std::env::temp_dir().join("obrain-test-modelo-incompleto");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_present(&dir));
        std::fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        assert!(!is_present(&dir), "falta model.onnx");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn las_instrucciones_nombran_los_tres_archivos() {
        let s = manual_instructions(Path::new("D:/modelos"));
        for f in e5::FILES {
            assert!(s.contains(f), "faltan las ordenes para {f}");
        }
    }
}
