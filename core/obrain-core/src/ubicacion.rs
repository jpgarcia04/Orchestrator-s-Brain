//! **Dónde viven los datos de obrain**: la base, su llave y el modelo de la
//! búsqueda por significado ([ADR-0029]).
//!
//! En Windows, en `%USERPROFILE%\.obrain` —`C:\Users\pacox\.obrain`—, y **no**
//! en `%LOCALAPPDATA%\obrain`, que es donde vivían hasta el 2026-09-23.
//!
//! El porqué es un truco de Windows: a cada aplicación empaquetada (la app de
//! Claude, la de Codex…) le da **su propia copia de `AppData`**. Todo lo que se
//! lanza desde ellas —su terminal, la app de obrain abierta desde ahí, la orden
//! `obrain`— escribe lo nuevo en esa copia y ve las dos carpetas mezcladas. Lo
//! que se abre desde una PowerShell normal sólo ve la de verdad. Así la base de
//! agosto y el modelo acabaron en la copia de Claude, la app abierta desde una
//! PowerShell creó otra base sin decir nada, y la sesión de Claude vio dos
//! archivos llamados igual en la misma carpeta. Fuera de `AppData` no hay
//! copia: la carpeta es la misma la abra quien la abra. (Lo comprobó un
//! proceso lanzado fuera de Claude, que veía lo que Claude escribía en el
//! perfil y no lo que escribía en `AppData`.)
//!
//! La carpeta empieza por punto a propósito: el escáner ya poda las carpetas
//! con punto de la raíz del perfil (son de programas, no tuyas), así que la
//! base no entra en tu bóveda aunque tu perfil entero sea una biblioteca.
//!
//! [ADR-0029]: ../../../docs/adr/0029-una-carpeta-fija-fuera-de-appdata.md

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// El nombre de la base dentro de la carpeta.
pub const BASE: &str = "brain.obrain";

/// Los modelos, dentro de la carpeta.
pub const MODELOS: &str = "models";

/// La carpeta de obrain, **sin mirar** si hay algo que mudar.
///
/// Windows: `%USERPROFILE%\.obrain`. Fuera de Windows no hay copias por
/// aplicación y se queda donde estaba: `$XDG_DATA_HOME/obrain` o
/// `~/.local/share/obrain`.
pub fn fija() -> PathBuf {
    #[cfg(windows)]
    {
        let perfil = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        perfil.join(".obrain")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
            .join("obrain")
    }
}

/// Donde vivían los datos antes de ADR-0029: `%LOCALAPPDATA%\obrain`. Sólo en
/// Windows.
pub fn vieja() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("obrain"))
    } else {
        None
    }
}

/// La carpeta de obrain, **con lo de la carpeta vieja ya mudado**. La mudanza
/// se intenta una vez por proceso, la primera vez que alguien pregunta.
///
/// Si la base vieja no se pudo mover —la tiene abierta una obrain de antes—,
/// esta vez se usa donde está: abrir la carpeta nueva crearía una base vacía al
/// lado, que es justo el fallo que esto arregla. La próxima vez se vuelve a
/// intentar.
pub fn carpeta() -> PathBuf {
    static ELEGIDA: OnceLock<PathBuf> = OnceLock::new();
    ELEGIDA
        .get_or_init(|| {
            let nueva = fija();
            let Some(vieja) = vieja() else {
                return nueva;
            };
            match mudar(&vieja, &nueva) {
                Ok(m) => {
                    if m.base > 0 || m.modelos {
                        eprintln!("▸ obrain vive ahora en {}", nueva.display());
                    }
                    if m.en_las_dos {
                        eprintln!(
                            "▸ aviso: hay otra base en {} que ya no se usa; la de {} es la buena",
                            vieja.display(),
                            nueva.display()
                        );
                    }
                    nueva
                }
                Err(e) if vieja.join(BASE).exists() && !nueva.join(BASE).exists() => {
                    eprintln!(
                        "▸ no se pudo mudar la base a {} ({e}); esta vez se abre en {}",
                        nueva.display(),
                        vieja.display()
                    );
                    vieja
                }
                Err(e) => {
                    eprintln!("▸ no se pudo mudar a {}: {e}", nueva.display());
                    nueva
                }
            }
        })
        .clone()
}

/// La carpeta de los modelos. Sin mudar nada: la de la carpeta fija si existe,
/// si no la de la vieja si existe, y si no ninguna, la de la fija, que es donde
/// se descargan.
pub fn modelos() -> PathBuf {
    let aqui = fija().join(MODELOS);
    if !aqui.exists() {
        if let Some(alli) = vieja().map(|v| v.join(MODELOS)).filter(|p| p.exists()) {
            return alli;
        }
    }
    aqui
}

/// Lo que hizo una mudanza.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mudanza {
    /// Archivos de la base que se movieron: ella, su llave, su WAL…
    pub base: usize,
    /// Se movió la carpeta de los modelos.
    pub modelos: bool,
    /// Había base en las dos carpetas. No se tocó ninguna: la nueva manda.
    pub en_las_dos: bool,
}

fn con_sufijo(p: &Path, sufijo: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(sufijo);
    PathBuf::from(s)
}

/// Muda de `vieja` a `nueva` lo que falte en `nueva`: la base con su WAL, su
/// memoria compartida y su llave, y la carpeta de los modelos. No pisa nada
/// que ya esté en `nueva`, y se puede repetir: si se corta a medias, la
/// siguiente termina.
///
/// **La base, lo primero, y ella la última de los suyos.** Si otra obrain la
/// tiene abierta, Windows no deja mover su WAL, la mudanza para ahí sin haber
/// movido nada, y quien llama la sigue usando donde está. Y si hay que cortar
/// entre medias, que sea con la base todavía en su sitio.
pub fn mudar(vieja: &Path, nueva: &Path) -> io::Result<Mudanza> {
    let mut m = Mudanza::default();
    if !vieja.is_dir() {
        return Ok(m);
    }
    let (base_vieja, base_nueva) = (vieja.join(BASE), nueva.join(BASE));
    if base_nueva.exists() {
        m.en_las_dos = base_vieja.exists();
    } else {
        for sufijo in ["-wal", "-shm", ".llave", ""] {
            let (de, a) = (
                con_sufijo(&base_vieja, sufijo),
                con_sufijo(&base_nueva, sufijo),
            );
            if de.exists() && !a.exists() {
                std::fs::create_dir_all(nueva)?;
                mover(&de, &a)?;
                m.base += 1;
            }
        }
    }
    let (de, a) = (vieja.join(MODELOS), nueva.join(MODELOS));
    if de.is_dir() && !a.exists() {
        std::fs::create_dir_all(nueva)?;
        mover(&de, &a)?;
        m.modelos = true;
    }
    Ok(m)
}

/// Renombrar, y si el destino está en otro disco, copiar y borrar. Nada más:
/// si el origen está abierto, el error sube y no se copia una base viva.
fn mover(de: &Path, a: &Path) -> io::Result<()> {
    match std::fs::rename(de, a) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            copiar(de, a)?;
            if de.is_dir() {
                std::fs::remove_dir_all(de)
            } else {
                std::fs::remove_file(de)
            }
        }
        r => r,
    }
}

fn copiar(de: &Path, a: &Path) -> io::Result<()> {
    if de.is_dir() {
        std::fs::create_dir_all(a)?;
        for e in std::fs::read_dir(de)? {
            let e = e?;
            copiar(&e.path(), &a.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(de, a).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una carpeta de prueba vacía, distinta para cada test.
    fn tmp(nombre: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("obrain-ubicacion-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn escribir(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    #[test]
    fn muda_la_base_con_su_llave_y_su_wal_y_los_modelos() {
        let d = tmp("todo");
        let (vieja, nueva) = (d.join("AppData").join("obrain"), d.join(".obrain"));
        escribir(&vieja.join("brain.obrain"), "base");
        escribir(&vieja.join("brain.obrain.llave"), "llave");
        escribir(&vieja.join("brain.obrain-wal"), "wal");
        escribir(
            &vieja.join("models").join("e5").join("model.onnx"),
            "modelo",
        );
        // Lo que no es de la base se queda donde está.
        escribir(&vieja.join("vida.obrain"), "otra");

        let m = mudar(&vieja, &nueva).unwrap();
        assert_eq!((m.base, m.modelos, m.en_las_dos), (3, true, false));
        for (f, s) in [
            ("brain.obrain", "base"),
            ("brain.obrain.llave", "llave"),
            ("brain.obrain-wal", "wal"),
        ] {
            assert_eq!(std::fs::read_to_string(nueva.join(f)).unwrap(), s);
            assert!(!vieja.join(f).exists(), "{f} sigue en la vieja");
        }
        assert!(nueva.join("models").join("e5").join("model.onnx").is_file());
        assert!(vieja.join("vida.obrain").is_file());
        assert!(!nueva.join("vida.obrain").exists());

        // Otra vez: nada que hacer.
        assert_eq!(mudar(&vieja, &nueva).unwrap(), Mudanza::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn una_base_que_ya_esta_en_la_nueva_no_se_pisa() {
        let d = tmp("dos");
        let (vieja, nueva) = (d.join("vieja"), d.join("nueva"));
        escribir(&vieja.join("brain.obrain"), "vieja");
        escribir(&vieja.join("brain.obrain.llave"), "llave vieja");
        escribir(&nueva.join("brain.obrain"), "nueva");

        let m = mudar(&vieja, &nueva).unwrap();
        assert!(m.en_las_dos);
        assert_eq!(m.base, 0);
        assert_eq!(
            std::fs::read_to_string(nueva.join("brain.obrain")).unwrap(),
            "nueva"
        );
        // Ni siquiera la llave: la de la vieja no abre la nueva.
        assert!(!nueva.join("brain.obrain.llave").exists());
        assert!(vieja.join("brain.obrain.llave").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn una_mudanza_cortada_a_medias_termina_la_vez_siguiente() {
        let d = tmp("medias");
        let (vieja, nueva) = (d.join("vieja"), d.join("nueva"));
        // Se cortó después de mover la llave y antes de mover la base.
        escribir(&nueva.join("brain.obrain.llave"), "llave");
        escribir(&vieja.join("brain.obrain"), "base");

        let m = mudar(&vieja, &nueva).unwrap();
        assert_eq!(m.base, 1);
        assert_eq!(
            std::fs::read_to_string(nueva.join("brain.obrain")).unwrap(),
            "base"
        );
        assert_eq!(
            std::fs::read_to_string(nueva.join("brain.obrain.llave")).unwrap(),
            "llave"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sin_carpeta_vieja_no_hace_nada() {
        let d = tmp("nada");
        let m = mudar(&d.join("no-existe"), &d.join("nueva")).unwrap();
        assert_eq!(m, Mudanza::default());
        assert!(
            !d.join("nueva").exists(),
            "no crea la carpeta si no hay nada"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// En Windows, una base abierta no se puede mover: la mudanza para sin
    /// tocar nada y la base se sigue usando donde está.
    #[cfg(windows)]
    #[test]
    fn una_base_abierta_no_se_muda_ni_a_medias() {
        let d = tmp("abierta");
        let (vieja, nueva) = (d.join("vieja"), d.join("nueva"));
        std::fs::create_dir_all(&vieja).unwrap();
        let conn = rusqlite::Connection::open(vieja.join("brain.obrain")).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; CREATE TABLE t (x); INSERT INTO t VALUES (1);",
        )
        .unwrap();
        escribir(&vieja.join("brain.obrain.llave"), "llave");

        assert!(mudar(&vieja, &nueva).is_err());
        for f in ["brain.obrain", "brain.obrain-wal", "brain.obrain.llave"] {
            assert!(vieja.join(f).exists(), "{f} tenía que seguir en la vieja");
            assert!(!nueva.join(f).exists(), "{f} no tenía que moverse");
        }
        drop(conn);
        let _ = std::fs::remove_dir_all(&d);
    }
}
