//! Identidad de archivos (invariante I8).
//!
//! **La ruta no es la identidad.** Un archivo que se mueve o se renombra sigue
//! siendo el mismo archivo, y todo el conocimiento acumulado sobre él tiene que
//! sobrevivir a ese movimiento. La identidad es:
//!
//! - `(volumen, FileID)` — estable ante mover y renombrar dentro del volumen.
//! - `BLAKE3` del contenido — estable ante copiar entre volúmenes, y lo que
//!   permite detectar duplicados y versiones.
//!
//! Las rutas pasan a ser **observaciones históricas**: "estuvo aquí en esta fecha".

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Identidad de un archivo en el sistema de ficheros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub volume: u64,
    /// `FILE_ID_128`: 128 bits, válido tanto en NTFS como en ReFS.
    pub file_id: [u8; 16],
}

impl FileIdentity {
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(49);
        s.push_str(&format!("{:016x}:", self.volume));
        for b in self.file_id {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// Abre un archivo pidiendo **sólo atributos**.
///
/// Es la diferencia entre identificar y descargar: con `FILE_READ_ATTRIBUTES`
/// y sin acceso a datos, un archivo `RECALL_ON_DATA_ACCESS` de OneDrive se
/// puede abrir sin provocar tráfico. Pedir `GENERIC_READ` haría lo mismo,
/// pero deja la puerta abierta a que alguien lea por error más adelante.
#[cfg(windows)]
fn open_attributes_only(path: &Path) -> std::io::Result<File> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// Obtiene `(volumen, FileID)` sin leer un solo byte de contenido.
///
/// El llamante **debe** haber comprobado antes `Availability::may_open()`.
/// Para un `CloudPlaceholder` (`RECALL_ON_OPEN`) esta llamada dispararía la
/// descarga, y por eso nunca debe alcanzarse con uno.
#[cfg(windows)]
pub fn file_identity(path: &Path) -> std::io::Result<FileIdentity> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO};

    let file = open_attributes_only(path)?;
    let mut info: FILE_ID_INFO = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as _,
            FileIdInfo,
            &mut info as *mut _ as *mut core::ffi::c_void,
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(FileIdentity {
        volume: info.VolumeSerialNumber,
        file_id: info.FileId.Identifier,
    })
}

#[cfg(not(windows))]
pub fn file_identity(path: &Path) -> std::io::Result<FileIdentity> {
    // Fuera de Windows usamos (dispositivo, inodo), que es el equivalente.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(path)?;
        let mut file_id = [0u8; 16];
        file_id[..8].copy_from_slice(&m.ino().to_le_bytes());
        Ok(FileIdentity { volume: m.dev(), file_id })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "identidad de archivo no soportada en esta plataforma",
        ))
    }
}

/// Hash BLAKE3 del contenido.
///
/// **Lee el archivo entero.** El llamante debe haber comprobado
/// `Availability::may_read_content()`: llamar a esto sobre un archivo
/// deshidratado de OneDrive lo descarga.
pub fn content_hash(path: &Path) -> std::io::Result<String> {
    const BUF: usize = 128 * 1024;
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; BUF];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, contents: &[u8]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("obrain-test-{name}"));
        let mut f = File::create(&p).unwrap();
        f.write_all(contents).unwrap();
        p
    }

    #[test]
    fn el_hash_depende_del_contenido_no_del_nombre() {
        let a = temp_file("hash-a.txt", b"nomina marzo 2023");
        let b = temp_file("hash-b.txt", b"nomina marzo 2023");
        let c = temp_file("hash-c.txt", b"otra cosa");

        let ha = content_hash(&a).unwrap();
        assert_eq!(ha, content_hash(&b).unwrap(), "mismo contenido, mismo hash");
        assert_ne!(ha, content_hash(&c).unwrap());
        assert_eq!(ha.len(), 64);

        for p in [a, b, c] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn la_identidad_sobrevive_al_renombrado() {
        let original = temp_file("identidad-antes.txt", b"contenido");
        let id_antes = file_identity(&original).unwrap();

        let mut destino = std::env::temp_dir();
        destino.push("obrain-test-identidad-despues.txt");
        let _ = std::fs::remove_file(&destino);
        std::fs::rename(&original, &destino).unwrap();

        let id_despues = file_identity(&destino).unwrap();
        assert_eq!(
            id_antes, id_despues,
            "renombrar no crea un archivo nuevo (invariante I8)"
        );

        let _ = std::fs::remove_file(destino);
    }

    #[test]
    fn dos_archivos_distintos_tienen_identidades_distintas() {
        let a = temp_file("id-1.txt", b"a");
        let b = temp_file("id-2.txt", b"a");
        assert_ne!(file_identity(&a).unwrap(), file_identity(&b).unwrap());
        // ...pero el mismo hash, que es justo como se detectan duplicados.
        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
        for p in [a, b] {
            let _ = std::fs::remove_file(p);
        }
    }
}
