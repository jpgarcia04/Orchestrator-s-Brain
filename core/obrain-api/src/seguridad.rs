//! **Seguridad**: la base cifrada y el PIN ([ADR-0021]).
//!
//! Dos cosas distintas que se confunden fácil, y por eso viven juntas y se
//! explican juntas:
//!
//! - **El cifrado** protege la base **en el disco**. Es SQLCipher con una llave
//!   que guarda Windows (`obrain-keys`). Abrir es transparente; crear la llave y
//!   cifrar una base que estaba en claro pasa aquí, en [`abrir`].
//! - **El PIN** protege **ver**, dentro de la aplicación. No cifra nada: decide
//!   si el contenido de lo sensible sale del núcleo hacia la ventana. Su hash
//!   vive dentro de la base cifrada.
//!
//! [ADR-0021]: ../../../docs/adr/0021-cifrado-en-reposo-y-pin.md

use obrain_core::rusqlite::{Connection, OptionalExtension, params};
use obrain_core::Store;
use obrain_keys::{KdfParams, RecoveryCode};
use std::path::{Path, PathBuf};

/// Lo que hay que saber de la seguridad de la base, para enseñarlo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Estado {
    /// La base está cifrada en el disco.
    pub cifrada: bool,
    /// Quién guarda la llave: `dpapi` (Windows) o `sin-proteger` (el banco de
    /// pruebas). `None` si la base no está cifrada.
    pub proteccion: Option<String>,
    /// Hay una clave de recuperación que la persona todavía no ha confirmado
    /// haber guardado. La app la enseña hasta que lo confirme.
    pub recuperacion_pendiente: bool,
    /// Hay PIN.
    pub pin: bool,
    /// Con PIN: el contenido sensible se puede ver ahora mismo.
    pub desbloqueada: bool,
    /// Con PIN y tras varios fallos: cuánto falta para poder volver a probar.
    pub espera_ms: i64,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] obrain_core::Error),

    #[error(transparent)]
    Keys(#[from] obrain_keys::Error),

    #[error(transparent)]
    Sqlite(#[from] obrain_core::rusqlite::Error),

    #[error("e/s: {0}")]
    Io(#[from] std::io::Error),

    #[error("cifrar la base no dejó lo mismo que había: {0}")]
    MigracionIncompleta(String),

    #[error("el PIN son de {PIN_MIN} a {PIN_MAX} números")]
    PinMalFormado,

    #[error("el PIN no es ése")]
    PinIncorrecto,

    #[error("demasiados intentos: espera {0} s")]
    PinEnEspera(i64),

    #[error("esta base no tiene PIN")]
    SinPin,

    #[error("esta base no está cifrada: no tiene clave de recuperación")]
    SinCifrar,
}

pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 8;

const DDL: &str = "
CREATE TABLE IF NOT EXISTS seguridad (
    clave TEXT PRIMARY KEY,
    valor TEXT NOT NULL
);";

/// La tabla de ajustes de seguridad. No es conocimiento —no va al log— y vive
/// dentro de la base, así que está cifrada con ella.
pub fn migrate(conn: &Connection) -> obrain_core::rusqlite::Result<()> {
    conn.execute_batch(DDL)
}

fn leer(conn: &Connection, clave: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT valor FROM seguridad WHERE clave = ?1", [clave], |r| r.get(0))
        .optional()?)
}

fn poner(conn: &Connection, clave: &str, valor: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO seguridad (clave, valor) VALUES (?1, ?2)
         ON CONFLICT(clave) DO UPDATE SET valor = excluded.valor",
        params![clave, valor],
    )?;
    Ok(())
}

fn quitar(conn: &Connection, clave: &str) -> Result<()> {
    conn.execute("DELETE FROM seguridad WHERE clave = ?1", [clave])?;
    Ok(())
}

fn con_sufijo(db: &Path, sufijo: &str) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(sufijo);
    PathBuf::from(s)
}

// ═════════════════════════════════════════════════════════════════ el cifrado

/// **La entrada del producto a la base.** Siempre la deja cifrada:
///
/// - si no existe, la crea cifrada;
/// - si existe en claro, la **cifra en el sitio** (y lo que había se sustituye);
/// - si ya está cifrada, la abre con la llave de Windows.
///
/// Cuando crea o cifra, guarda la clave de recuperación **dentro** de la base y
/// la marca como pendiente: la app la enseña hasta que la persona confirme que
/// la guardó ([`confirmar_recuperacion`]).
///
/// Si Windows no puede abrir la llave, el error es
/// [`obrain_keys::Error::SistemaNoDisponible`] y la salida es [`recuperar`].
pub fn abrir(db: &Path) -> Result<Store> {
    abrir_con(db, KdfParams::DEFAULT)
}

/// [`abrir`] con otro coste de Argon2id. Sólo para los tests.
pub fn abrir_con(db: &Path, kdf: KdfParams) -> Result<Store> {
    if obrain_keys::exists(db) {
        let cifrando = con_sufijo(db, ".cifrando");
        if cifrando.exists() {
            reanudar(db, &cifrando)?;
        }
        let store = Store::open(db)?;
        migrate(store.conn())?;
        return Ok(store);
    }
    if !db.exists() {
        let c = obrain_keys::create_with(db, kdf)?;
        let store = Store::open_with(db, Some(&c.key))?;
        migrate(store.conn())?;
        guardar_recuperacion(store.conn(), &c.recovery)?;
        return Ok(store);
    }
    cifrar(db, kdf)
}

/// Cifra una base que estaba en claro, en el sitio.
///
/// El orden es lo que la hace segura ante un corte de luz a mitad:
///
/// 1. copia cifrada en `<base>.cifrando`, con la clave de recuperación dentro;
/// 2. comprobación: lo mismo en las dos;
/// 3. **se escribe la llave** — desde aquí la base «está cifrada»;
/// 4. la copia sustituye a la original.
///
/// Si se corta entre 3 y 4, la próxima apertura ve la llave y la copia y
/// termina el paso 4 ([`reanudar`]). Si se corta antes de 3, no ha pasado
/// nada: la original sigue en claro y la copia a medias se tira.
fn cifrar(db: &Path, kdf: KdfParams) -> Result<Store> {
    let cifrando = con_sufijo(db, ".cifrando");
    for resto in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(con_sufijo(&cifrando, resto));
    }
    // La original en claro, abierta y puesta al día antes de copiarla: una base
    // de una versión anterior gana aquí las tablas nuevas.
    let antes = {
        let s = Store::open_with(db, None)?;
        migrate(s.conn())?;
        cuenta(s.conn())?
    };

    let llave = obrain_keys::prepare(kdf)?;
    Store::export_encrypted(db, &cifrando, llave.key())?;
    {
        let s = Store::open_with(&cifrando, Some(llave.key()))?;
        let despues = cuenta(s.conn())?;
        if antes != despues {
            let _ = std::fs::remove_file(&cifrando);
            return Err(Error::MigracionIncompleta(format!("{antes:?} → {despues:?}")));
        }
        guardar_recuperacion(s.conn(), llave.recovery())?;
    }
    llave.write(db)?;
    reanudar(db, &cifrando)?;

    let store = Store::open(db)?;
    migrate(store.conn())?;
    Ok(store)
}

/// El paso 4: la copia cifrada ocupa el sitio de la original.
fn reanudar(db: &Path, cifrando: &Path) -> Result<()> {
    // Antes de sustituir nada, que la copia se abra con la llave que ya está
    // escrita: si no, se deja todo como está y se dice.
    {
        let s = Store::open_with(cifrando, Some(&obrain_keys::unlock(db)?))?;
        // Todo dentro del archivo principal: lo que se mueve es sólo ése.
        s.conn().query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    }
    for resto in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(con_sufijo(cifrando, resto));
        let _ = std::fs::remove_file(con_sufijo(db, resto));
    }
    std::fs::rename(cifrando, db)?;
    Ok(())
}

/// Lo que tiene que coincidir entre la original y la copia.
fn cuenta(conn: &Connection) -> Result<(i64, i64, i64)> {
    let n = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    Ok((
        n("SELECT count(*) FROM entities")?,
        n("SELECT count(*) FROM assertions")?,
        n("SELECT count(*) FROM sqlite_master")?,
    ))
}

fn guardar_recuperacion(conn: &Connection, code: &RecoveryCode) -> Result<()> {
    migrate(conn)?;
    poner(conn, "recuperacion", &code.to_string())?;
    quitar(conn, "recuperacion_guardada")
}

/// Abre la llave con la clave de recuperación y la vuelve a dejar en manos de
/// Windows. Es la salida cuando [`abrir`] dice que este equipo no puede.
pub fn recuperar(db: &Path, codigo: &str) -> Result<()> {
    let code = RecoveryCode::parse(codigo)?;
    obrain_keys::recover(db, &code)?;
    Ok(())
}

/// La clave de recuperación, para volver a enseñarla. Está dentro de la base
/// cifrada: quien puede leerla ya podía leer todo lo demás.
pub fn clave_de_recuperacion(conn: &Connection) -> Result<Option<String>> {
    leer(conn, "recuperacion")
}

/// La persona dice que ya la guardó: la app deja de insistir.
pub fn confirmar_recuperacion(conn: &Connection) -> Result<()> {
    poner(conn, "recuperacion_guardada", "1")
}

/// ¿Es ésta la clave de recuperación de la base abierta en `conn`?
pub fn comprobar_recuperacion(conn: &Connection, codigo: &str) -> Result<()> {
    let db = conn.path().filter(|p| !p.is_empty()).map(PathBuf::from).ok_or(Error::SinCifrar)?;
    if !obrain_keys::exists(&db) {
        return Err(Error::SinCifrar);
    }
    obrain_keys::verify_recovery(&db, &RecoveryCode::parse(codigo)?)?;
    Ok(())
}

/// Lo que la app enseña de la seguridad, menos el desbloqueo, que es de la
/// sesión y lo pone [`crate::Brain`].
pub fn estado(conn: &Connection, ahora_ms: i64) -> Result<Estado> {
    let db = conn.path().filter(|p| !p.is_empty()).map(PathBuf::from);
    let proteccion = match &db {
        Some(d) if obrain_keys::exists(d) => Some(obrain_keys::protection(d)?),
        _ => None,
    };
    let espera = leer(conn, "pin_espera_hasta")?
        .and_then(|v| v.parse::<i64>().ok())
        .map_or(0, |h| (h - ahora_ms).max(0));
    Ok(Estado {
        cifrada: proteccion.is_some(),
        recuperacion_pendiente: leer(conn, "recuperacion")?.is_some()
            && leer(conn, "recuperacion_guardada")?.is_none(),
        proteccion,
        pin: leer(conn, "pin")?.is_some(),
        desbloqueada: false,
        espera_ms: espera,
    })
}

// ═════════════════════════════════════════════════════════════════════ el PIN

/// Cuánto hay que esperar tras `fallos` intentos equivocados seguidos: nada
/// los dos primeros, y desde el tercero 30 s que se doblan hasta 15 minutos.
pub fn espera_tras(fallos: u32) -> i64 {
    if fallos < 3 {
        return 0;
    }
    let pasos = (fallos - 3).min(10);
    (30_000i64 << pasos).min(15 * 60_000)
}

fn validar(pin: &str) -> Result<()> {
    if (PIN_MIN..=PIN_MAX).contains(&pin.len()) && pin.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(Error::PinMalFormado)
    }
}

pub fn tiene_pin(conn: &Connection) -> Result<bool> {
    Ok(leer(conn, "pin")?.is_some())
}

/// Pone (o cambia) el PIN. Comprobar el anterior es cosa de quien llama.
pub fn poner_pin(conn: &Connection, pin: &str) -> Result<()> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    validar(pin)?;
    let mut sal = [0u8; 16];
    getrandom::fill(&mut sal).map_err(|e| Error::MigracionIncompleta(e.to_string()))?;
    let sal = SaltString::encode_b64(&sal).map_err(|e| Error::MigracionIncompleta(e.to_string()))?;
    let hash = argon2::Argon2::default()
        .hash_password(pin.as_bytes(), &sal)
        .map_err(|e| Error::MigracionIncompleta(e.to_string()))?
        .to_string();
    poner(conn, "pin", &hash)?;
    quitar(conn, "pin_fallos")?;
    quitar(conn, "pin_espera_hasta")
}

pub fn quitar_pin(conn: &Connection) -> Result<()> {
    for c in ["pin", "pin_fallos", "pin_espera_hasta"] {
        quitar(conn, c)?;
    }
    Ok(())
}

/// Comprueba un PIN, **con espera creciente tras los fallos**. La espera se
/// guarda en la base, así que cerrar y abrir la app no la salta.
pub fn comprobar_pin(conn: &Connection, pin: &str, ahora_ms: i64) -> Result<()> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    let hash = leer(conn, "pin")?.ok_or(Error::SinPin)?;
    if let Some(hasta) = leer(conn, "pin_espera_hasta")?.and_then(|v| v.parse::<i64>().ok()) {
        if hasta > ahora_ms {
            return Err(Error::PinEnEspera((hasta - ahora_ms + 999) / 1000));
        }
    }
    let bueno = PasswordHash::new(&hash)
        .map(|h| argon2::Argon2::default().verify_password(pin.as_bytes(), &h).is_ok())
        .unwrap_or(false);
    if bueno {
        quitar(conn, "pin_fallos")?;
        quitar(conn, "pin_espera_hasta")?;
        return Ok(());
    }
    let fallos = leer(conn, "pin_fallos")?.and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) + 1;
    poner(conn, "pin_fallos", &fallos.to_string())?;
    let espera = espera_tras(fallos);
    if espera > 0 {
        poner(conn, "pin_espera_hasta", &(ahora_ms + espera).to_string())?;
    }
    Err(Error::PinIncorrecto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_espera_crece_y_tiene_techo() {
        assert_eq!(espera_tras(1), 0);
        assert_eq!(espera_tras(2), 0);
        assert_eq!(espera_tras(3), 30_000);
        assert_eq!(espera_tras(4), 60_000);
        assert_eq!(espera_tras(5), 120_000);
        assert_eq!(espera_tras(40), 15 * 60_000);
    }

    #[test]
    fn un_pin_son_de_4_a_8_numeros() {
        assert!(validar("1234").is_ok());
        assert!(validar("12345678").is_ok());
        assert!(validar("123").is_err());
        assert!(validar("123456789").is_err());
        assert!(validar("12a4").is_err());
    }
}
