//! La caja del sistema: la llave, protegida por el sistema operativo.
//!
//! En Windows es **DPAPI** en el ámbito del usuario: sólo tu cuenta de Windows,
//! en este equipo, la puede abrir, y la abre sin preguntar. Fuera de Windows no
//! hay nada equivalente que no pida una contraseña, y como la v1 es sólo
//! Windows (F4), la caja se guarda sin proteger y lo dice.

use crate::{DataKey, Error, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Caja {
    /// `dpapi` o `sin-proteger`. Una caja se abre sólo con el sistema que la
    /// cerró: una caja de DPAPI en Linux no es un error de formato, es «hace
    /// falta la clave de recuperación».
    pub tipo: String,
    pub datos: String,
}

#[cfg(windows)]
pub(crate) const TIPO: &str = "dpapi";
#[cfg(not(windows))]
pub(crate) const TIPO: &str = "sin-proteger";

pub(crate) fn wrap(key: &DataKey) -> Result<Caja> {
    Ok(Caja {
        tipo: TIPO.to_string(),
        datos: B64.encode(proteger(key.bytes())?),
    })
}

pub(crate) fn unwrap(caja: &Caja) -> Result<DataKey> {
    if caja.tipo != TIPO {
        return Err(Error::SistemaNoDisponible(format!(
            "la llave se guardó con «{}» y este sistema usa «{TIPO}»",
            caja.tipo
        )));
    }
    let datos = B64
        .decode(&caja.datos)
        .map_err(|e| Error::Formato(e.to_string()))?;
    let llave = desproteger(&datos)?;
    DataKey::from_slice(&llave)
}

// ─────────────────────────────────────────────────────────────────── Windows

#[cfg(windows)]
fn proteger(datos: &[u8]) -> Result<Vec<u8>> {
    dpapi::proteger(datos)
}

#[cfg(windows)]
fn desproteger(datos: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    dpapi::desproteger(datos)
}

#[cfg(windows)]
mod dpapi {
    use crate::{Error, Result};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };
    use zeroize::Zeroizing;

    /// Entropía propia: otra aplicación que guarde algo con DPAPI no puede
    /// hacer pasar su caja por la nuestra, ni al revés.
    const ENTROPIA: &[u8] = b"obrain-llave-v1";

    fn blob(b: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: b.len() as u32,
            pbData: b.as_ptr() as *mut u8,
        }
    }

    fn vacio() -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        }
    }

    fn fallo(que: &str) -> Error {
        Error::SistemaNoDisponible(format!("{que}: {}", std::io::Error::last_os_error()))
    }

    pub(super) fn proteger(datos: &[u8]) -> Result<Vec<u8>> {
        let entrada = blob(datos);
        let entropia = blob(ENTROPIA);
        let mut salida = vacio();
        // SAFETY: los punteros de entrada viven durante la llamada; la salida
        // la reserva DPAPI con LocalAlloc y se libera abajo con LocalFree.
        let ok = unsafe {
            CryptProtectData(
                &entrada,
                std::ptr::null(),
                &entropia,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut salida,
            )
        };
        if ok == 0 {
            return Err(fallo("DPAPI no pudo proteger la llave"));
        }
        // SAFETY: DPAPI devolvió `cbData` bytes válidos en `pbData`.
        let v =
            unsafe { std::slice::from_raw_parts(salida.pbData, salida.cbData as usize) }.to_vec();
        // SAFETY: `pbData` lo reservó DPAPI con LocalAlloc.
        unsafe { LocalFree(salida.pbData.cast()) };
        Ok(v)
    }

    pub(super) fn desproteger(datos: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        let entrada = blob(datos);
        let entropia = blob(ENTROPIA);
        let mut salida = vacio();
        // SAFETY: como arriba.
        let ok = unsafe {
            CryptUnprotectData(
                &entrada,
                std::ptr::null_mut(),
                &entropia,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut salida,
            )
        };
        if ok == 0 {
            return Err(fallo("Windows no pudo abrir la llave"));
        }
        // SAFETY: DPAPI devolvió `cbData` bytes válidos en `pbData`, y se
        // ponen a cero antes de liberarlos: es la llave en claro.
        let v = unsafe {
            let s = std::slice::from_raw_parts_mut(salida.pbData, salida.cbData as usize);
            let copia = Zeroizing::new(s.to_vec());
            std::ptr::write_bytes(salida.pbData, 0, salida.cbData as usize);
            LocalFree(salida.pbData.cast());
            copia
        };
        Ok(v)
    }
}

// ─────────────────────────────────────────────────────── fuera de Windows

#[cfg(not(windows))]
fn proteger(datos: &[u8]) -> Result<Vec<u8>> {
    Ok(datos.to_vec())
}

#[cfg(not(windows))]
fn desproteger(datos: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    Ok(zeroize::Zeroizing::new(datos.to_vec()))
}
