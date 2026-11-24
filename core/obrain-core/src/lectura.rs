//! **Abrir para leer**: la apertura de la puerta de Aetheris (ADR-0038, «Qué
//! hay que construir»; ADR-0039, condición 3).
//!
//! [`Store::open`] no es una apertura de lectura: pone la base en WAL, crea
//! tablas, migra y siembra predicados. Esta apertura no hace nada de eso ni
//! puede hacerlo: la conexión es `SQLITE_OPEN_READ_ONLY` con
//! `PRAGMA query_only`, nunca crea un archivo y nunca recupera un log. Si la
//! base pide cualquier cosa que no sea leerla tal como está, se niega con un
//! [`NoDisponible`] y no toca nada.
//!
//! ## Leer sin tocar ningún archivo
//!
//! Abrir en sólo lectura una base en WAL **no** basta para no escribir. Medido
//! con SQLCipher en Linux (2026-11-24):
//!
//! | Archivos junto a la base | Apertura normal | `readonly_shm=1` | `immutable=1` |
//! |---|---|---|---|
//! | ninguno (nadie la tiene abierta) | crea `-wal` y `-shm`, y se quedan | falla, y deja un `-wal` vacío | lee, no crea nada |
//! | `-wal` y `-shm` (otro proceso la tiene) | lee | lee | **no ve** lo del log |
//! | `-wal` y `-shm` de un corte (nadie los tiene) | reescribe `-shm` | lee sin tocar nada | no ve lo del log |
//! | sólo `-wal` | — | falla | — |
//!
//! Así que hay dos modos y nada más:
//!
//! - **Con log** (`-wal` y `-shm` presentes): `readonly_shm=1`. SQLite lee el
//!   log que tiene la ventana, o el que quedó de un corte, sin escribir en él;
//!   si no puede leerlo así, falla, y eso es [`NoDisponible::Ocupada`].
//! - **Sin log** (ninguno de los dos): `immutable=1`, que no toma bloqueos ni
//!   mira el log. Es correcto sólo si nadie escribe mientras se lee, así que
//!   se toma la huella del archivo antes de abrir y se comprueba **después de
//!   cada lectura** ([`Lectura::leer`]): si apareció un log o el archivo
//!   cambió, lo leído se tira y la respuesta es `Ocupada`.
//!
//! Cualquier otra combinación (un log sin su `-shm`, un `-shm` sin log, un
//! `-journal`) necesita recuperación, que sólo hace la ventana: `Ocupada`.
//!
//! `-shm` no es contenido: es el índice del log en memoria compartida. Una
//! conexión de sólo lectura del mismo proceso que la ventana comparte el suyo
//! y puede apuntar en él su marca de lectura; la de otro proceso lo abre en
//! sólo lectura. La base, el log y la llave no cambian nunca.
//!
//! ## La puerta no retiene la base
//!
//! Una [`Lectura`] es para una llamada: se abre, se lee y se suelta. Mientras
//! vive, en modo «con log», restaurar un respaldo diría «base en uso»; en modo
//! «sin log» no toma bloqueos, así que una restauración puede ocurrir mientras
//! lee, y la huella lo detecta.

use crate::error::{Error, Result};
use crate::opening;
use crate::schema::SCHEMA_VERSION;
use crate::store::Store;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Por qué la puerta no puede leer la base ahora. Son los motivos de
/// `no_disponible` del ADR-0038 que dependen de la base; `apagada` es del
/// ajuste y no se decide aquí.
///
/// El texto es para el registro de quien llama, nunca para un modelo: dice
/// qué archivo o qué versión, no qué hay dentro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoDisponible {
    /// No hay base, no hay llave, la llave no la abre este sistema o la base
    /// no se deja leer con ella.
    BaseCerrada(String),
    /// Una restauración, un cifrado, una mudanza o una escritura a medias, un
    /// log que necesita recuperación, o algo que cambió mientras se leía.
    Ocupada(String),
    /// La base necesita una migración que sólo hace la ventana, o es de una
    /// versión más nueva que esta puerta.
    VersionDistinta(String),
}

impl NoDisponible {
    /// El motivo tal como viaja en la respuesta de la puerta.
    pub fn motivo(&self) -> &'static str {
        match self {
            NoDisponible::BaseCerrada(_) => "base_cerrada",
            NoDisponible::Ocupada(_) => "ocupada",
            NoDisponible::VersionDistinta(_) => "version_distinta",
        }
    }
}

impl std::fmt::Display for NoDisponible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (NoDisponible::BaseCerrada(d)
        | NoDisponible::Ocupada(d)
        | NoDisponible::VersionDistinta(d)) = self;
        write!(f, "{}: {d}", self.motivo())
    }
}

impl std::error::Error for NoDisponible {}

/// Lo que puede salir mal en una lectura ya abierta: o la base dejó de estar
/// disponible mientras se leía, o la consulta falló.
#[derive(Debug)]
pub enum Fallo {
    NoDisponible(NoDisponible),
    Error(Error),
}

impl std::fmt::Display for Fallo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fallo::NoDisponible(n) => n.fmt(f),
            Fallo::Error(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Fallo {}

/// Tamaño y fecha de modificación del archivo de la base: lo que cambia si
/// alguien escribe en él mientras se lee en modo «sin log».
#[derive(Debug, Clone, PartialEq, Eq)]
struct Huella {
    len: u64,
    modified: Option<SystemTime>,
}

fn huella(path: &Path) -> std::io::Result<Huella> {
    let m = std::fs::metadata(path)?;
    Ok(Huella {
        len: m.len(),
        modified: m.modified().ok(),
    })
}

#[derive(Debug)]
enum Modo {
    ConLog,
    SinLog(Huella),
}

/// Una base abierta para leer, para una sola llamada. Ver el módulo.
pub struct Lectura {
    store: Store,
    path: PathBuf,
    modo: Modo,
}

impl std::fmt::Debug for Lectura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lectura")
            .field("path", &self.path)
            .field("modo", &self.modo)
            .finish_non_exhaustive()
    }
}

fn ocupada(d: impl Into<String>) -> NoDisponible {
    NoDisponible::Ocupada(d.into())
}

fn cerrada(d: impl Into<String>) -> NoDisponible {
    NoDisponible::BaseCerrada(d.into())
}

/// ¿Dice este fallo «otro la tiene» o «hace falta recuperar el log»? Ocupado,
/// bloqueado, no se pudo abrir el log, y los `SQLITE_READONLY_*` con código
/// extendido (`RECOVERY`, `CANTINIT`, `CANTLOCK`…) que da `readonly_shm`
/// cuando no puede leer el log tal como está. **No** el `SQLITE_READONLY`
/// simple, que es un intento de escribir: eso es un error de quien llama y no
/// se disfraza de ocupación.
fn es_ocupacion(e: &rusqlite::Error) -> bool {
    use rusqlite::ErrorCode::*;
    match e {
        rusqlite::Error::SqliteFailure(f, _) => match f.code {
            DatabaseBusy | DatabaseLocked | CannotOpen => true,
            ReadOnly => f.extended_code != rusqlite::ffi::SQLITE_READONLY,
            _ => false,
        },
        _ => false,
    }
}

/// Un fallo de SQLite al abrir, traducido: ocupación es `Ocupada`; lo demás,
/// que la base no se deja leer.
fn de_sqlite(e: &rusqlite::Error) -> NoDisponible {
    if es_ocupacion(e) {
        ocupada(format!("sqlite: {e}"))
    } else {
        cerrada(format!("sqlite: {e}"))
    }
}

fn existe(path: &Path, sufijo: &str) -> bool {
    opening::suffix(path, sufijo).exists()
}

impl Store {
    /// Abre `path` sólo para leer, con su llave, sin crear, cifrar, migrar ni
    /// reparar nada. `tablas` son las que quien llama necesita además de las
    /// del log (las proyecciones viven en otros crates): si falta alguna, la
    /// base no está al día y la respuesta es `VersionDistinta`.
    ///
    /// No mira los restos de una restauración (`brain.obrain.transicion.json`),
    /// que son de `obrain-api`: eso lo comprueba quien llama, antes.
    pub fn abrir_para_leer(
        path: impl AsRef<Path>,
        tablas: &[&str],
    ) -> std::result::Result<Lectura, NoDisponible> {
        let path = path.as_ref();
        // Lo que pide una escritura a medias, primero: sin mirar si la base
        // está, porque una restauración o un cifrado cortados pueden haberla
        // quitado de su sitio.
        if opening::has_migration_artifacts(path) {
            return Err(ocupada("hay restos de un cifrado o de una llave a medias"));
        }
        if existe(path, "-journal") {
            return Err(ocupada("hay un -journal: una escritura quedó a medias"));
        }
        match std::fs::metadata(path) {
            Ok(m) if m.is_file() && m.len() > 0 => {}
            Ok(_) => return Err(cerrada("la base está vacía o no es un archivo")),
            Err(_) => return Err(cerrada("no hay base")),
        }
        if !obrain_keys::exists(path) {
            // Una base en claro se cifra al abrirla la ventana (ADR-0021); la
            // puerta no lee nada que no esté cifrado.
            return Err(cerrada("la base no tiene llave"));
        }
        let modo = match (existe(path, "-wal"), existe(path, "-shm")) {
            (true, true) => Modo::ConLog,
            (false, false) => Modo::SinLog(huella(path).map_err(|e| cerrada(format!("e/s: {e}")))?),
            (true, false) => return Err(ocupada("hay un log sin su -shm: necesita recuperación")),
            (false, true) => return Err(ocupada("hay un -shm sin su log")),
        };
        let key = obrain_keys::unlock(path).map_err(|e| cerrada(format!("llave: {e}")))?;

        let parametro = match modo {
            Modo::ConLog => "readonly_shm=1",
            Modo::SinLog(_) => "immutable=1",
        };
        let conn = Connection::open_with_flags(
            uri(path, parametro),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| de_sqlite(&e))?;
        // Antes que nada que pueda leer: ni siquiera una sentencia mal escrita
        // de quien llama puede escribir con esta conexión.
        conn.execute_batch("PRAGMA query_only = ON;")
            .map_err(|e| de_sqlite(&e))?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| de_sqlite(&e))?;
        match opening::key(&conn, Some(&key)) {
            Ok(()) => {}
            Err(Error::WrongKey) => return Err(cerrada("la base no se abre con su llave")),
            Err(Error::Sqlite(e)) => return Err(de_sqlite(&e)),
            Err(e) => return Err(cerrada(e.to_string())),
        }
        drop(key);

        let lectura = Lectura {
            store: Store::de_lectura(conn),
            path: path.to_path_buf(),
            modo,
        };
        lectura
            .leer(|store| Ok(comprobar_estructura(store.conn(), tablas)))
            .map_err(|f| match f {
                Fallo::NoDisponible(n) => n,
                Fallo::Error(Error::Sqlite(e)) => de_sqlite(&e),
                Fallo::Error(e) => cerrada(e.to_string()),
            })??;
        Ok(lectura)
    }
}

/// La URI de SQLite para `path` con un parámetro. Los caracteres que una URI
/// reserva se escapan: una ruta de Windows con `#` o `?` no puede cortar la
/// URI ni añadirle parámetros.
fn uri(path: &Path, parametro: &str) -> String {
    let texto = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:");
    if texto.starts_with(|c: char| c.is_ascii_alphabetic()) && texto.get(1..2) == Some(":") {
        // `C:/…` necesita la barra inicial para ser una ruta absoluta.
        out.push('/');
    }
    for c in texto.chars() {
        match c {
            '%' | '?' | '#' | ' ' => out.push_str(&format!("%{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('?');
    out.push_str(parametro);
    out
}

/// La versión es exactamente la de este programa y están todas las tablas que
/// se van a leer. Más vieja la sube la ventana; más nueva, otra puerta.
fn comprobar_estructura(
    conn: &Connection,
    tablas: &[&str],
) -> std::result::Result<(), NoDisponible> {
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| de_sqlite(&e))?;
    if version != SCHEMA_VERSION {
        return Err(NoDisponible::VersionDistinta(format!(
            "la base es de la versión {version} y la puerta lee la {SCHEMA_VERSION}"
        )));
    }
    for tabla in LOG.iter().chain(tablas) {
        let hay: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1",
                [tabla],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| de_sqlite(&e))?;
        if hay.is_none() {
            return Err(NoDisponible::VersionDistinta(format!(
                "falta la tabla {tabla}: la ventana la crea al abrir"
            )));
        }
    }
    Ok(())
}

/// Las tablas del log que esta apertura necesita siempre.
const LOG: &[&str] = &[
    "entities",
    "predicates",
    "assertions",
    "content_publication",
];

impl Lectura {
    /// Lee dentro de una transacción de lectura y, en modo «sin log»,
    /// comprueba **después** que nadie escribió mientras tanto. Si alguien lo
    /// hizo, el resultado se tira: `Ocupada`.
    ///
    /// Es la única forma de llegar al [`Store`]: no hay otra por la que leer y
    /// olvidar la comprobación.
    pub fn leer<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> std::result::Result<T, Fallo> {
        let conn = self.store.conn();
        conn.execute_batch("BEGIN")
            .map_err(|e| Fallo::NoDisponible(de_sqlite(&e)))?;
        let resultado = f(&self.store);
        // Cerrar la transacción de lectura no escribe; si falla, la conexión
        // ya no sirve y la siguiente llamada abre otra.
        let fin = conn.execute_batch("COMMIT");
        self.sigue_igual().map_err(Fallo::NoDisponible)?;
        let valor = resultado.map_err(|e| match e {
            Error::Sqlite(ref s) if es_ocupacion(s) => Fallo::NoDisponible(de_sqlite(s)),
            e => Fallo::Error(e),
        })?;
        fin.map_err(|e| Fallo::NoDisponible(de_sqlite(&e)))?;
        Ok(valor)
    }

    /// En modo «sin log»: no apareció un log y el archivo no cambió.
    fn sigue_igual(&self) -> std::result::Result<(), NoDisponible> {
        let Modo::SinLog(antes) = &self.modo else {
            return Ok(());
        };
        if existe(&self.path, "-wal")
            || existe(&self.path, "-shm")
            || existe(&self.path, "-journal")
        {
            return Err(ocupada("otro proceso abrió la base mientras se leía"));
        }
        match huella(&self.path) {
            Ok(ahora) if &ahora == antes => Ok(()),
            _ => Err(ocupada("la base cambió mientras se leía")),
        }
    }

    /// `true` si se abrió leyendo el log de otra conexión (`readonly_shm`);
    /// `false` si la base estaba sola (`immutable`). Para las pruebas y el
    /// registro.
    pub fn con_log(&self) -> bool {
        matches!(self.modo, Modo::ConLog)
    }
}
