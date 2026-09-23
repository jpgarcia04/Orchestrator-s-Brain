//! # obrain-keys — la llave de la base · ADR-0021
//!
//! La base se cifra entera con SQLCipher bajo una llave de **256 bits
//! aleatorios**. Esa llave no se escribe nunca en claro: vive en un archivo
//! pequeño junto a la base, `<base>.llave`, **dos veces**, cada una en su caja:
//!
//! ```text
//!   brain.obrain.llave
//!   ├── sistema        la llave, cifrada por Windows (DPAPI)      ← a diario
//!   └── recuperacion   la llave, cifrada con la clave de          ← si Windows
//!                      recuperación (Argon2id + XChaCha20)           la pierde
//! ```
//!
//! A diario abre la caja del sistema, sin preguntar nada: la app y `obrain
//! indexar` abren solas con tu sesión de Windows. Si Windows ya no puede
//! abrirla —otro equipo, Windows reinstalado, una contraseña restablecida por un
//! administrador—, la clave de recuperación abre la otra caja y rehace la del
//! sistema ([`recover`]).
//!
//! Fuera de Windows no hay DPAPI, y la caja del sistema se guarda **sin
//! proteger** (el archivo lo dice: `"tipo": "sin-proteger"`). No es un modo de
//! uso: es lo que permite que los tests y el banco de pruebas corran en Linux.
//!
//! Este crate no sabe nada de SQLite. Entrega una llave; quien abre la base
//! decide qué hacer con ella.

mod codigo;
mod sistema;

pub use codigo::RecoveryCode;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// La extensión del archivo de la llave, que va junto a la base.
pub const EXTENSION: &str = "llave";

/// Lo que se autentica junto a la llave en la caja de recuperación: una caja
/// de otra cosa, o de otra versión, no se abre por accidente.
const AAD: &[u8] = b"obrain-llave-v1";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("e/s: {0}")]
    Io(#[from] std::io::Error),

    #[error("el archivo de la llave está dañado: {0}")]
    Formato(String),

    #[error(
        "este equipo no puede abrir la llave de la base ({0}); hace falta la clave de recuperación"
    )]
    SistemaNoDisponible(String),

    #[error("esa no es la clave de recuperación de esta base")]
    ClaveIncorrecta,

    #[error(
        "una clave de recuperación son 25 letras y números (los guiones y los espacios dan igual)"
    )]
    CodigoMalFormado,

    #[error("ya existe una llave para esta base")]
    YaExiste,

    #[error("no se pudo generar azar del sistema: {0}")]
    Azar(String),
}

// ─────────────────────────────────────────────────────────────────── la llave

/// La llave de la base: 256 bits aleatorios. Se borra de la memoria al soltarse.
pub struct DataKey(Zeroizing<[u8; 32]>);

impl DataKey {
    fn random() -> Result<Self> {
        let mut k = Zeroizing::new([0u8; 32]);
        azar(k.as_mut())?;
        Ok(DataKey(k))
    }

    fn from_slice(b: &[u8]) -> Result<Self> {
        let arr: [u8; 32] = b
            .try_into()
            .map_err(|_| Error::Formato(format!("la llave mide {} bytes, no 32", b.len())))?;
        Ok(DataKey(Zeroizing::new(arr)))
    }

    fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// La llave como la entiende SQLCipher cuando se le da en crudo:
    /// `x'2DD2…'`. En crudo no hay derivación al abrir: la llave ya tiene
    /// 256 bits de azar y derivarla otra vez sólo costaría tiempo.
    pub fn sqlcipher_literal(&self) -> Zeroizing<String> {
        let mut s = Zeroizing::new(String::with_capacity(67));
        s.push_str("x'");
        for b in self.0.iter() {
            s.push(
                char::from_digit(u32::from(b >> 4), 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
            s.push(
                char::from_digit(u32::from(b & 0x0f), 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
        }
        s.push('\'');
        s
    }
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DataKey(…)")
    }
}

fn azar(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|e| Error::Azar(e.to_string()))
}

// ─────────────────────────────────────────────────────────── el archivo

/// Dónde vive la llave de esta base: `brain.obrain` → `brain.obrain.llave`.
pub fn key_path(db: &Path) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(".");
    s.push(EXTENSION);
    PathBuf::from(s)
}

/// ¿Esta base tiene llave? Si la tiene, está cifrada.
pub fn exists(db: &Path) -> bool {
    key_path(db).is_file()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct KeyFile {
    version: u32,
    /// Con qué se cifra la base. Hoy sólo `sqlcipher-4`.
    cifrado: String,
    sistema: sistema::Caja,
    recuperacion: CajaRecuperacion,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CajaRecuperacion {
    kdf: String,
    /// Memoria en KiB, pasadas y paralelismo de Argon2id. Viajan con la caja
    /// para poder endurecerlos sin dejar de abrir las cajas viejas.
    m: u32,
    t: u32,
    p: u32,
    sal: String,
    nonce: String,
    datos: String,
}

/// Cuánto cuesta derivar la clave de recuperación.
#[derive(Debug, Clone, Copy)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl KdfParams {
    /// 64 MiB y 3 pasadas: unas décimas de segundo en una laptop, y sólo se
    /// paga al crear la llave y al recuperarla.
    pub const DEFAULT: KdfParams = KdfParams {
        m_kib: 64 * 1024,
        t: 3,
        p: 1,
    };
    /// Para los tests: el mínimo que acepta Argon2. **Nunca** en una base real.
    pub const FAST_FOR_TESTS: KdfParams = KdfParams {
        m_kib: 8,
        t: 1,
        p: 1,
    };
}

/// Lo que devuelve crear una llave: la llave, y la clave de recuperación que
/// hay que enseñarle a la persona **una vez**.
#[derive(Debug)]
pub struct Created {
    pub key: DataKey,
    pub recovery: RecoveryCode,
}

/// Crea la llave de una base que todavía no la tiene.
pub fn create(db: &Path) -> Result<Created> {
    create_with(db, KdfParams::DEFAULT)
}

pub fn create_with(db: &Path, kdf: KdfParams) -> Result<Created> {
    if exists(db) {
        return Err(Error::YaExiste);
    }
    prepare(kdf)?.write(db)
}

/// Una llave nueva que **todavía no está escrita**.
///
/// Existe para cifrar una base que estaba en claro: primero se hace la copia
/// cifrada con esta llave y se comprueba, y sólo entonces se escribe el
/// archivo. Una llave escrita antes de tener su base sería una base en claro
/// que ya no se abre.
pub struct Pending {
    created: Created,
    archivo: KeyFile,
}

impl Pending {
    pub fn key(&self) -> &DataKey {
        &self.created.key
    }

    pub fn recovery(&self) -> &RecoveryCode {
        &self.created.recovery
    }

    /// Escribe la llave junto a la base. A partir de aquí, la base está cifrada.
    pub fn write(self, db: &Path) -> Result<Created> {
        write(db, &self.archivo)?;
        Ok(self.created)
    }
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pending(…)")
    }
}

/// Genera una llave y su clave de recuperación sin escribir nada.
pub fn prepare(kdf: KdfParams) -> Result<Pending> {
    let key = DataKey::random()?;
    let recovery = RecoveryCode::generate()?;
    let archivo = KeyFile {
        version: 1,
        cifrado: "sqlcipher-4".into(),
        sistema: sistema::wrap(&key)?,
        recuperacion: wrap_recovery(&key, &recovery, kdf)?,
    };
    Ok(Pending {
        created: Created { key, recovery },
        archivo,
    })
}

/// La llave, abierta por el sistema. Es lo que pasa cada vez que se abre la
/// base.
pub fn unlock(db: &Path) -> Result<DataKey> {
    sistema::unwrap(&read(db)?.sistema)
}

/// Abre la llave con la clave de recuperación y **rehace la caja del
/// sistema**, de modo que la próxima vez vuelve a abrir sola.
pub fn recover(db: &Path, code: &RecoveryCode) -> Result<DataKey> {
    let mut archivo = read(db)?;
    let key = unwrap_recovery(&archivo.recuperacion, code)?;
    archivo.sistema = sistema::wrap(&key)?;
    write(db, &archivo)?;
    Ok(key)
}

/// ¿Es ésta la clave de recuperación de la base? Sin tocar nada. Es lo que
/// pide restablecer un PIN olvidado.
pub fn verify_recovery(db: &Path, code: &RecoveryCode) -> Result<()> {
    unwrap_recovery(&read(db)?.recuperacion, code).map(|_| ())
}

/// Cómo está guardada la caja del sistema: `dpapi` en Windows,
/// `sin-proteger` en el banco de pruebas.
pub fn protection(db: &Path) -> Result<String> {
    Ok(read(db)?.sistema.tipo)
}

fn read(db: &Path) -> Result<KeyFile> {
    let texto = std::fs::read_to_string(key_path(db))?;
    let archivo: KeyFile =
        serde_json::from_str(&texto).map_err(|e| Error::Formato(e.to_string()))?;
    if archivo.version != 1 || archivo.cifrado != "sqlcipher-4" {
        return Err(Error::Formato(format!(
            "versión {} con «{}»: esta versión de obrain no la sabe abrir",
            archivo.version, archivo.cifrado
        )));
    }
    Ok(archivo)
}

/// Escribe en un temporal y lo pone en su sitio con un `rename`: una llave a
/// medio escribir sería una base que no se abre.
fn write(db: &Path, archivo: &KeyFile) -> Result<()> {
    let destino = key_path(db);
    let mut tmp = destino.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let json = serde_json::to_string_pretty(archivo).map_err(|e| Error::Formato(e.to_string()))?;
    {
        let mut opciones = std::fs::OpenOptions::new();
        opciones.write(true).create(true).truncate(true);
        // Fuera de Windows la caja del sistema no protege nada: que por lo
        // menos el archivo sólo lo pueda leer su dueño.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opciones.mode(0o600);
        }
        let mut f = opciones.open(&tmp)?;
        std::io::Write::write_all(&mut f, json.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &destino)?;
    Ok(())
}

// ─────────────────────────────────────────────────── la caja de recuperación

fn derive(code: &RecoveryCode, sal: &[u8], m: u32, t: u32, p: u32) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(m, t, p, Some(32)).map_err(|e| Error::Formato(e.to_string()))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(code.normalized().as_bytes(), sal, out.as_mut())
        .map_err(|e| Error::Formato(e.to_string()))?;
    Ok(out)
}

fn wrap_recovery(key: &DataKey, code: &RecoveryCode, kdf: KdfParams) -> Result<CajaRecuperacion> {
    let mut sal = [0u8; 16];
    let mut nonce = [0u8; 24];
    azar(&mut sal)?;
    azar(&mut nonce)?;
    let kek = derive(code, &sal, kdf.m_kib, kdf.t, kdf.p)?;
    let caja = XChaCha20Poly1305::new(kek.as_ref().into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: key.bytes(),
                aad: AAD,
            },
        )
        .map_err(|_| Error::Formato("no se pudo cifrar la caja de recuperación".into()))?;
    Ok(CajaRecuperacion {
        kdf: "argon2id".into(),
        m: kdf.m_kib,
        t: kdf.t,
        p: kdf.p,
        sal: B64.encode(sal),
        nonce: B64.encode(nonce),
        datos: B64.encode(caja),
    })
}

fn unwrap_recovery(c: &CajaRecuperacion, code: &RecoveryCode) -> Result<DataKey> {
    if c.kdf != "argon2id" {
        return Err(Error::Formato(format!("derivación desconocida: {}", c.kdf)));
    }
    let b64 = |s: &str| B64.decode(s).map_err(|e| Error::Formato(e.to_string()));
    let (sal, nonce, datos) = (b64(&c.sal)?, b64(&c.nonce)?, b64(&c.datos)?);
    if nonce.len() != 24 {
        return Err(Error::Formato("nonce de tamaño incorrecto".into()));
    }
    let kek = derive(code, &sal, c.m, c.t, c.p)?;
    let llave = Zeroizing::new(
        XChaCha20Poly1305::new(kek.as_ref().into())
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &datos,
                    aad: AAD,
                },
            )
            // Un código equivocado y una caja manipulada fallan igual, y para
            // quien lo usa significan lo mismo: con esto no se abre.
            .map_err(|_| Error::ClaveIncorrecta)?,
    );
    DataKey::from_slice(&llave)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(dir: &Path) -> PathBuf {
        dir.join("brain.obrain")
    }

    fn tmp(nombre: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("obrain-keys-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn la_llave_vive_junto_a_la_base() {
        assert_eq!(
            key_path(Path::new("/x/brain.obrain")),
            PathBuf::from("/x/brain.obrain.llave")
        );
    }

    #[test]
    fn crear_y_abrir_da_la_misma_llave() {
        let d = tmp("crear");
        let db = base(&d);
        assert!(!exists(&db));
        let c = create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();
        assert!(exists(&db));
        let k = unlock(&db).unwrap();
        assert_eq!(k.bytes(), c.key.bytes());
        assert!(matches!(
            create_with(&db, KdfParams::FAST_FOR_TESTS),
            Err(Error::YaExiste)
        ));
    }

    /// La clave de recuperación abre la llave, y la equivocada no.
    #[test]
    fn la_clave_de_recuperacion_abre_y_la_otra_no() {
        let d = tmp("recuperar");
        let db = base(&d);
        let c = create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();
        let k = recover(&db, &c.recovery).unwrap();
        assert_eq!(k.bytes(), c.key.bytes());
        verify_recovery(&db, &c.recovery).unwrap();

        let otra = RecoveryCode::generate().unwrap();
        assert!(matches!(recover(&db, &otra), Err(Error::ClaveIncorrecta)));
        assert!(matches!(
            verify_recovery(&db, &otra),
            Err(Error::ClaveIncorrecta)
        ));
    }

    /// El caso para el que existe: la llave se guardó con un sistema que este
    /// equipo no tiene (en Linux se simula con una caja de DPAPI). No abre; la
    /// clave de recuperación sí, y rehace la caja para la próxima vez.
    #[test]
    fn otro_equipo_necesita_la_recuperacion_y_luego_abre_solo() {
        let d = tmp("otro-equipo");
        let db = base(&d);
        let c = create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();

        let ruta = key_path(&db);
        let otro_sistema = if cfg!(windows) {
            "sin-proteger"
        } else {
            "dpapi"
        };
        let texto = std::fs::read_to_string(&ruta).unwrap().replace(
            &format!("\"tipo\": \"{}\"", sistema::TIPO),
            &format!("\"tipo\": \"{otro_sistema}\""),
        );
        std::fs::write(&ruta, texto).unwrap();
        assert!(matches!(unlock(&db), Err(Error::SistemaNoDisponible(_))));

        recover(&db, &c.recovery).unwrap();
        assert_eq!(unlock(&db).unwrap().bytes(), c.key.bytes());
        assert_eq!(protection(&db).unwrap(), sistema::TIPO);
    }

    /// Ni la clave de recuperación ni la llave de SQLCipher aparecen tal cual
    /// en el archivo de la caja de recuperación.
    #[test]
    fn el_archivo_no_guarda_la_clave_de_recuperacion() {
        let d = tmp("archivo");
        let db = base(&d);
        let c = create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();
        let texto = std::fs::read_to_string(key_path(&db)).unwrap();
        assert!(!texto.contains(c.recovery.normalized()));
        assert!(!texto.contains(&c.recovery.to_string()));
    }

    /// Con los parámetros de verdad también funciona (es el único test que
    /// paga los 64 MiB de Argon2id).
    #[test]
    fn con_los_parametros_de_verdad_tambien() {
        let d = tmp("verdad");
        let db = base(&d);
        let c = create(&db).unwrap();
        verify_recovery(&db, &c.recovery).unwrap();
    }

    #[test]
    fn el_literal_de_sqlcipher_es_la_llave_en_hexadecimal() {
        let k = DataKey::from_slice(&[0xAB; 32]).unwrap();
        let l = k.sqlcipher_literal();
        assert_eq!(l.len(), 67);
        assert!(l.starts_with("x'ABAB") && l.ends_with("AB'"));
        assert_eq!(format!("{k:?}"), "DataKey(…)");
    }

    #[test]
    fn un_archivo_danado_se_dice() {
        let d = tmp("danado");
        let db = base(&d);
        std::fs::write(key_path(&db), "{ no es json").unwrap();
        assert!(matches!(unlock(&db), Err(Error::Formato(_))));
    }
}
