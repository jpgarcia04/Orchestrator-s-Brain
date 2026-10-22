//! **Respaldos consistentes y verificables** de la base cifrada.
//!
//! Un respaldo de Brain es una carpeta nueva con tres archivos que corresponden
//! a la **misma instantánea transaccional**:
//!
//! ```text
//!   <carpeta-respaldo>/
//!   ├── brain.obrain         instantánea SQLCipher consolidada (sin WAL)
//!   ├── brain.obrain.llave   caja del sistema (DPAPI) + caja de recuperación
//!   └── manifiesto.json      hashes, tamaños y resumen verificable sin datos personales
//! ```
//!
//! ## Qué garantiza y qué no
//!
//! - **Sí garantiza**: consistencia transaccional de SQLite/SQLCipher (incluyendo
//!   las transacciones ya confirmadas en el WAL en el momento de la instantánea),
//!   correspondencia criptográfica entre `brain.obrain`, `brain.obrain.llave` y
//!   `manifiesto.json`, integridad estructural y de páginas HMAC, ausencia de
//!   copias temporales en claro y publicación atómica sin sobrescribir respaldos
//!   anteriores.
//! - **No garantiza por sí solo**: que este respaldo sea la copia más reciente
//!   de una base activa si se siguieron escribiendo transacciones después sin
//!   una referencia externa independiente, ni incluye los documentos originales
//!   externos de las bibliotecas del usuario (PDF, imágenes, contratos en sus
//!   carpetas originales), que deben respaldarse por separado.

use crate::seguridad;
use obrain_core::Store;
use obrain_core::rusqlite::{Connection, OpenFlags};
use obrain_keys::RecoveryCode;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Nombre del archivo de base dentro de un respaldo.
pub const BASE_ARCHIVO: &str = "brain.obrain";

/// Nombre del archivo de llave dentro de un respaldo.
pub const LLAVE_ARCHIVO: &str = "brain.obrain.llave";

/// Nombre del manifiesto dentro de un respaldo.
pub const MANIFIESTO_ARCHIVO: &str = "manifiesto.json";

/// Identificador de formato del manifiesto.
pub const FORMATO_MANIFIESTO: &str = "obrain-respaldo-v1";

/// Versión actual del esquema de manifiesto.
pub const VERSION_MANIFIESTO: u32 = 1;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] obrain_core::Error),

    #[error(transparent)]
    Keys(#[from] obrain_keys::Error),

    #[error(transparent)]
    Seguridad(#[from] seguridad::Error),

    #[error(transparent)]
    Sqlite(#[from] obrain_core::rusqlite::Error),

    #[error("e/s: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "el destino {0} ya existe o no está vacío; elige una carpeta nueva para no sobrescribir un respaldo anterior"
    )]
    DestinoOcupado(PathBuf),

    #[error("el destino no es válido: {0}")]
    DestinoInvalido(String),

    #[error(
        "la base {0} no tiene archivo .llave; ábrela primero con Brain para cifrarla antes de crear un respaldo"
    )]
    OrigenSinCifrar(PathBuf),

    #[error(
        "la base {0} tiene restos de una conversión o escritura de llave pendiente; ábrela primero para terminar la recuperación"
    )]
    OrigenConRestos(PathBuf),

    #[error("falta un archivo requerido en el respaldo: {0}")]
    FaltaArchivo(PathBuf),

    #[error("un archivo del respaldo está vacío: {0}")]
    ArchivoVacio(PathBuf),

    #[error("el respaldo contiene un archivo auxiliar o desconocido: {0}")]
    ArchivoInesperado(PathBuf),

    #[error("el manifiesto del respaldo no es válido: {0}")]
    ManifiestoInvalido(String),

    #[error(
        "el tamaño de {archivo} no coincide con el manifiesto (esperado {esperado} bytes, encontrado {actual} bytes)"
    )]
    TamanoNoCoincide {
        archivo: String,
        esperado: u64,
        actual: u64,
    },

    #[error(
        "el hash de {archivo} no coincide con el manifiesto (esperado {esperado}, encontrado {actual})"
    )]
    HashNoCoincide {
        archivo: String,
        esperado: String,
        actual: String,
    },

    #[error("el archivo .llave y la base del respaldo no corresponden entre sí: {0}")]
    LlaveNoCorresponde(String),

    #[error("el contenido de la instantánea no coincide con el manifiesto: {0}")]
    InstantaneaNoCoincide(String),

    #[error("generación de respaldo interrumpida en etapa de prueba: {0}")]
    Interrumpido(String),

    #[error(
        "la base activa {0} está en uso por otro proceso o conexión; ciérrala antes de restaurar"
    )]
    BaseEnUso(PathBuf),

    #[error(
        "la restauración requiere autorización explícita; revisa el plan de sustitución y confirma con --confirmar"
    )]
    NoAutorizado,

    #[error("falló la restauración ({0}); se recuperó el estado anterior desde la salvaguarda")]
    RestauracionFallidaYRecuperada(String),

    #[error(
        "falló la restauración ({0}) y no se pudo recuperar la salvaguarda ({1}); los archivos originales se conservan en {2}"
    )]
    RestauracionCritica(String, String, PathBuf),

    #[error(
        "se detectó una restauración previa interrumpida en {0}; se recuperó desde la salvaguarda"
    )]
    TransicionInterrumpidaRecuperada(PathBuf),

    #[error(
        "se detectó una transición interrumpida con estado ambiguo en {0}; se detuvo la recuperación automática para preservar las evidencias"
    )]
    TransicionAmbigua(PathBuf),
}

/// Metadatos verificables de un archivo del respaldo.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArchivoRespaldo {
    pub nombre: String,
    pub bytes: u64,
    pub blake3: String,
}

/// Metadatos verificables del archivo `.llave` del respaldo.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LlaveRespaldo {
    pub nombre: String,
    pub bytes: u64,
    pub blake3: String,
    /// Hash BLAKE3 de los parámetros canónicos de la caja de recuperación.
    /// No expone secretos y permite autenticar la caja de recuperación.
    pub huella_recuperacion: String,
}

/// Comprobaciones de contenido obtenidas de la misma instantánea cifrada.
/// No incluye rutas, nombres de archivos ni textos personales.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResumenInstantanea {
    pub esquema_version: i32,
    pub entidades: i64,
    pub aserciones: i64,
    pub aserciones_vigentes: i64,
    pub predicados: i64,
    pub fragmentos: i64,
    pub vectores: i64,
    pub publicaciones_listas: i64,
    pub publicaciones_pendientes: i64,
    pub tiene_pin: bool,
    pub tiene_recuperacion_interna: bool,
    /// Resumen BLAKE3 canónico del log, estado de publicación y proyecciones
    /// dentro de la instantánea.
    pub huella_contenido: String,
}

/// Manifiesto de verificación de un respaldo.
///
/// Verifica la integridad y correspondencia mutua de los archivos de esa
/// instantánea; **no** demuestra por sí solo que no existan transacciones
/// posteriores en otra copia sin una referencia independiente.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Manifiesto {
    pub version: u32,
    pub formato: String,
    pub creado_en_ms: i64,
    pub base: ArchivoRespaldo,
    pub llave: LlaveRespaldo,
    pub instantanea: ResumenInstantanea,
}

/// Resultado de ensayar la apertura y recuperación aislada de un respaldo.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InformeEnsayo {
    pub manifiesto: Manifiesto,
    pub carpeta_ensayo: Option<PathBuf>,
    pub recuperada_con_clave: bool,
    pub con_pin: bool,
    pub pin_desbloqueado: bool,
    pub documentos_en_boveda: usize,
    pub en_papelera: usize,
    pub colecciones: usize,
    pub vencimientos: usize,
    pub sensibles: usize,
    pub boveda: usize,
    pub bibliotecas_registradas: Vec<String>,
    pub bibliotecas_presentes: usize,
    pub bibliotecas_ausentes_en_disco: Vec<String>,
}

/// Nombre del archivo de recibo de transición de restauración.
pub const RECIBO_TRANSICION_ARCHIVO: &str = "brain.obrain.transicion.json";

/// Información de una biblioteca registrada en el respaldo y su presencia local.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BibliotecaInfo {
    pub ruta: String,
    pub existe_en_disco: bool,
}

/// Resumen verificable del estado de un perfil de Brain (activo o anterior).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResumenPerfil {
    pub existe: bool,
    pub cifrada: bool,
    pub abrible: bool,
    pub esquema_version: i32,
    pub entidades: i64,
    pub aserciones: i64,
    pub aserciones_vigentes: i64,
    pub fragmentos: i64,
    pub vectores: i64,
    pub tiene_pin: bool,
    pub huella_contenido: Option<String>,
    pub bibliotecas: Vec<String>,
    pub error_acceso: Option<String>,
}

/// Diagnóstico y plan previo a la sustitución de un perfil existente.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlanRestauracion {
    pub manifiesto_respaldo: Manifiesto,
    pub db_destino: PathBuf,
    pub perfil_actual: ResumenPerfil,
    pub bibliotecas_respaldo: Vec<BibliotecaInfo>,
    pub requiere_clave_recuperacion: bool,
    pub llave_compatible_sistema: bool,
    pub advertencias: Vec<String>,
}

/// Resultado de la operación de restauración.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InformeRestauracion {
    pub manifiesto: Manifiesto,
    pub db_destino: PathBuf,
    pub salvaguarda_creada: Option<PathBuf>,
    pub recuperada_con_clave: bool,
    pub reindexado_en_pausa: bool,
    pub bibliotecas_presentes: Vec<String>,
    pub bibliotecas_ausentes: Vec<String>,
}

/// Opciones de configuración para la restauración.
#[derive(Debug, Clone)]
pub struct OpcionesRestauracion {
    pub codigo_recuperacion: Option<String>,
    pub autorizar: bool,
    pub pausar_reindexado: bool,
    pub carpeta_salvaguarda: Option<PathBuf>,
}

impl Default for OpcionesRestauracion {
    fn default() -> Self {
        Self {
            codigo_recuperacion: None,
            autorizar: false,
            pausar_reindexado: true,
            carpeta_salvaguarda: None,
        }
    }
}

/// Etapas internas de restauración, expuestas para pruebas de interrupción y reversibilidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtapaRestauracion {
    TrasVerificarRespaldo,
    TrasInspeccionarActiva,
    TrasCrearSalvaguarda,
    TrasPrepararStaging,
    TrasDesplazarActiva,
    TrasSustituirArchivos,
    AntesDeValidarFinal,
}

/// Recibo de transición de restauración persistido en disco para recuperación ante fallos.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReciboTransicion {
    pub version: u32,
    pub creado_en_ms: i64,
    pub salvaguarda_dir: Option<PathBuf>,
    pub staging_dir: PathBuf,
    pub target_db: PathBuf,
    pub target_llave: PathBuf,
    pub staged_db_hash: String,
    pub staged_llave_hash: String,
    pub target_db_hash_anterior: Option<String>,
    pub target_llave_hash_anterior: Option<String>,
    pub estado: String,
}

/// Etapas internas de generación, expuestas para pruebas de interrupción.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtapaRespaldo {
    TrasCopiarLlave,
    TrasExportarBase,
    TrasEscribirManifiesto,
    AntesDePublicar,
}

/// Crea un respaldo consistente, cifrado y verificado de `db` en `destino`.
///
/// Genera todos los archivos en un directorio temporal privado junto a
/// `destino`, verifica el respaldo completo y sólo entonces lo publica
/// atómicamente. Nunca sobrescribe un respaldo o directorio no vacío.
pub fn crear(db: &Path, destino: &Path) -> Result<Manifiesto> {
    crear_con_etapa(db, destino, |_| Ok(()))
}

/// Variante de [`crear`] que permite inyectar una interrupción entre etapas
/// para comprobar que nunca se publica un respaldo parcial.
pub fn crear_con_etapa<F>(db: &Path, destino: &Path, mut al_avanzar: F) -> Result<Manifiesto>
where
    F: FnMut(EtapaRespaldo) -> Result<()>,
{
    validar_destino_nuevo(db, destino)?;
    validar_origen(db)?;

    let destino_abs = absoluta(destino);
    let parent = destino_abs.parent().ok_or_else(|| {
        Error::DestinoInvalido(format!(
            "no se puede determinar la carpeta padre de {}",
            destino.display()
        ))
    })?;
    std::fs::create_dir_all(parent)?;

    let staging = DirTemporal::new_in(parent, ".obrain-respaldo-tmp-")?;
    let staged_db = staging.path().join(BASE_ARCHIVO);
    let staged_llave = staging.path().join(LLAVE_ARCHIVO);
    let staged_manifiesto = staging.path().join(MANIFIESTO_ARCHIVO);

    // 1. Copiar y sincronizar la llave antes de desbloquearla: así verificamos
    //    exactamente los bytes de llave que viajarán en el respaldo.
    copiar_sincronizado(&obrain_keys::key_path(db), &staged_llave)?;
    al_avanzar(EtapaRespaldo::TrasCopiarLlave)?;

    let canon_recuperacion = obrain_keys::recovery_box_canonical(&staged_db)?;
    let huella_recuperacion = blake3::hash(canon_recuperacion.as_bytes())
        .to_hex()
        .to_string();
    let key = obrain_keys::unlock(&staged_db)?;

    // 2. Exportar una instantánea cifrada y consolidada mediante VACUUM INTO.
    Store::export_snapshot_encrypted(db, &staged_db, &key)?;
    sincronizar_archivo(&staged_db)?;
    al_avanzar(EtapaRespaldo::TrasExportarBase)?;

    // 3. Inspeccionar la instantánea generada en solo lectura para que todas
    //    las comprobaciones del manifiesto provengan de la misma instantánea.
    let instantanea = {
        let conn = Connection::open_with_flags(&staged_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        aplicar_llave_lectura(&conn, &key)?;
        let resumen = inspeccionar_instantanea(&conn)?;
        if resumen.tiene_recuperacion_interna {
            let codigo = seguridad::clave_de_recuperacion(&conn)?.ok_or_else(|| {
                Error::LlaveNoCorresponde(
                    "falta la clave de recuperación en la tabla seguridad".into(),
                )
            })?;
            let parsed = RecoveryCode::parse(&codigo)?;
            obrain_keys::verify_recovery_matches(&staged_db, &key, &parsed).map_err(|e| {
                Error::LlaveNoCorresponde(format!(
                    "la clave de recuperación interna no abre la caja .llave del respaldo: {e}"
                ))
            })?;
        }
        resumen
    };

    let base_meta = describir_archivo(&staged_db, BASE_ARCHIVO)?;
    let llave_meta = describir_archivo(&staged_llave, LLAVE_ARCHIVO)?;
    let manifiesto = Manifiesto {
        version: VERSION_MANIFIESTO,
        formato: FORMATO_MANIFIESTO.to_string(),
        creado_en_ms: ahora_ms(),
        base: base_meta,
        llave: LlaveRespaldo {
            nombre: llave_meta.nombre,
            bytes: llave_meta.bytes,
            blake3: llave_meta.blake3,
            huella_recuperacion,
        },
        instantanea,
    };

    escribir_manifiesto(&staged_manifiesto, &manifiesto)?;
    al_avanzar(EtapaRespaldo::TrasEscribirManifiesto)?;

    // 4. Verificar el conjunto completo en el directorio temporal antes de publicarlo.
    let verificado = verificar_interno(staging.path(), None)?;
    if verificado != manifiesto {
        return Err(Error::InstantaneaNoCoincide(
            "la verificación previa a publicar devolvió un manifiesto distinto".into(),
        ));
    }
    al_avanzar(EtapaRespaldo::AntesDePublicar)?;

    // 5. Publicación atómica: si el destino era una carpeta vacía recién creada,
    //    se retira vacía antes del rename; si tiene cualquier archivo, se rechaza.
    publicar_staging(staging, &destino_abs)?;
    Ok(manifiesto)
}

/// Verifica la completitud, hashes, llave e integridad interna de un respaldo
/// utilizando la llave del sistema actual, sin modificar ningún archivo.
pub fn verificar(carpeta_respaldo: &Path) -> Result<Manifiesto> {
    verificar_con_clave(carpeta_respaldo, None)
}

/// Verifica un respaldo completo. Si se proporciona `codigo_recuperacion`,
/// descifra la llave desde la caja de recuperación (útil en otro equipo donde
/// DPAPI no puede abrir la caja del sistema) sin modificar el archivo `.llave`.
pub fn verificar_con_clave(
    carpeta_respaldo: &Path,
    codigo_recuperacion: Option<&str>,
) -> Result<Manifiesto> {
    if !carpeta_respaldo.exists() || !carpeta_respaldo.is_dir() {
        return Err(Error::DestinoInvalido(format!(
            "no existe la carpeta de respaldo {}",
            carpeta_respaldo.display()
        )));
    }

    if let Some(nombre) = carpeta_respaldo.file_name().and_then(|n| n.to_str()) {
        if nombre.starts_with(".obrain-respaldo-tmp-") || nombre.starts_with(".obrain-ensayo-tmp-")
        {
            return Err(Error::DestinoInvalido(format!(
                "la carpeta {} corresponde a un directorio temporal de trabajo no publicado",
                carpeta_respaldo.display()
            )));
        }
    }

    verificar_interno(carpeta_respaldo, codigo_recuperacion)
}

fn verificar_interno(
    carpeta_respaldo: &Path,
    codigo_recuperacion: Option<&str>,
) -> Result<Manifiesto> {
    // 1. Rechazar archivos auxiliares o desconocidos en la carpeta del respaldo.
    for entry in std::fs::read_dir(carpeta_respaldo)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            return Err(Error::ArchivoInesperado(path));
        };
        if !matches!(name_str, BASE_ARCHIVO | LLAVE_ARCHIVO | MANIFIESTO_ARCHIVO) {
            return Err(Error::ArchivoInesperado(path));
        }
        if !path.is_file() {
            return Err(Error::ArchivoInesperado(path));
        }
    }

    let manifiesto_path = carpeta_respaldo.join(MANIFIESTO_ARCHIVO);
    let base_path = carpeta_respaldo.join(BASE_ARCHIVO);
    let llave_path = carpeta_respaldo.join(LLAVE_ARCHIVO);

    for req in [&manifiesto_path, &base_path, &llave_path] {
        if !req.is_file() {
            return Err(Error::FaltaArchivo(req.clone()));
        }
        if req.metadata()?.len() == 0 {
            return Err(Error::ArchivoVacio(req.clone()));
        }
    }

    // 2. Leer y validar la estructura del manifiesto.
    let texto_manifiesto = std::fs::read_to_string(&manifiesto_path)?;
    let manifiesto: Manifiesto = serde_json::from_str(&texto_manifiesto)
        .map_err(|e| Error::ManifiestoInvalido(e.to_string()))?;
    if manifiesto.version != VERSION_MANIFIESTO || manifiesto.formato != FORMATO_MANIFIESTO {
        return Err(Error::ManifiestoInvalido(format!(
            "versión {} con formato «{}» no compatible",
            manifiesto.version, manifiesto.formato
        )));
    }
    if manifiesto.base.nombre != BASE_ARCHIVO {
        return Err(Error::ManifiestoInvalido(format!(
            "nombre de base inesperado: «{}»",
            manifiesto.base.nombre
        )));
    }
    if manifiesto.llave.nombre != LLAVE_ARCHIVO {
        return Err(Error::ManifiestoInvalido(format!(
            "nombre de llave inesperado: «{}»",
            manifiesto.llave.nombre
        )));
    }

    // 3. Comprobar tamaños y hashes BLAKE3 de los archivos antes de abrir SQLite.
    let base_actual = describir_archivo(&base_path, BASE_ARCHIVO)?;
    comprobar_archivo(&manifiesto.base, &base_actual)?;

    let llave_actual = describir_archivo(&llave_path, LLAVE_ARCHIVO)?;
    let llave_esperada = ArchivoRespaldo {
        nombre: manifiesto.llave.nombre.clone(),
        bytes: manifiesto.llave.bytes,
        blake3: manifiesto.llave.blake3.clone(),
    };
    comprobar_archivo(&llave_esperada, &llave_actual)?;

    let canon_recuperacion = obrain_keys::recovery_box_canonical(&base_path)?;
    let huella_actual = blake3::hash(canon_recuperacion.as_bytes())
        .to_hex()
        .to_string();
    if huella_actual != manifiesto.llave.huella_recuperacion {
        return Err(Error::HashNoCoincide {
            archivo: format!("{LLAVE_ARCHIVO} (recuperación)"),
            esperado: manifiesto.llave.huella_recuperacion.clone(),
            actual: huella_actual,
        });
    }

    // 4. Desbloquear la llave (sin modificar el archivo .llave del respaldo).
    let codigo_parseado = match codigo_recuperacion {
        Some(c) => Some(RecoveryCode::parse(c)?),
        None => None,
    };
    let key = match &codigo_parseado {
        Some(code) => obrain_keys::unlock_recovery(&base_path, code)?,
        None => obrain_keys::unlock(&base_path)?,
    };

    // 5. Verificar estructura, triggers, cipher_integrity_check, integrity_check
    //    y correspondencia de contenido sobre una copia privada desechable.
    Store::verify_snapshot(&base_path, &key).map_err(|e| match e {
        obrain_core::Error::WrongKey => Error::LlaveNoCorresponde(
            "la llave del respaldo no descifra la base brain.obrain".into(),
        ),
        other => Error::Core(other),
    })?;

    let private = DirTemporal::new("obrain-inspect-respaldo-")?;
    let copy_db = private.path().join(BASE_ARCHIVO);
    std::fs::copy(&base_path, &copy_db)?;
    let conn = Connection::open_with_flags(&copy_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    aplicar_llave_lectura(&conn, &key)?;

    let resumen_actual = inspeccionar_instantanea(&conn)?;
    if resumen_actual != manifiesto.instantanea {
        return Err(Error::InstantaneaNoCoincide(describir_diferencia(
            &manifiesto.instantanea,
            &resumen_actual,
        )));
    }

    if resumen_actual.tiene_recuperacion_interna {
        let codigo_interno = seguridad::clave_de_recuperacion(&conn)?.ok_or_else(|| {
            Error::InstantaneaNoCoincide(
                "falta la clave de recuperación en la tabla seguridad".into(),
            )
        })?;
        let internal_code = RecoveryCode::parse(&codigo_interno)?;
        obrain_keys::verify_recovery_matches(&base_path, &key, &internal_code).map_err(|e| {
            Error::LlaveNoCorresponde(format!(
                "la clave de recuperación interna no coincide con la caja de recuperación de .llave: {e}"
            ))
        })?;
        if let Some(provided) = &codigo_parseado {
            if provided.normalized() != internal_code.normalized() {
                return Err(Error::Keys(obrain_keys::Error::ClaveIncorrecta));
            }
        }
    }

    Ok(manifiesto)
}

/// Ensaya la recuperación y apertura completa de un respaldo en una ubicación
/// aislada (`destino_ensayo` o un directorio temporal descartable si es `None`),
/// sin tocar la base activa ni modificar los archivos del respaldo.
pub fn ensayar(
    carpeta_respaldo: &Path,
    destino_ensayo: Option<&Path>,
    codigo_recuperacion: Option<&str>,
    pin: Option<&str>,
) -> Result<InformeEnsayo> {
    if let Some(dest) = destino_ensayo {
        validar_destino_ensayo(carpeta_respaldo, dest)?;
    }

    let manifiesto = verificar_con_clave(carpeta_respaldo, codigo_recuperacion)?;

    let parent_temp = match destino_ensayo {
        Some(dest) => {
            let abs = absoluta(dest);
            let p = abs.parent().ok_or_else(|| {
                Error::DestinoInvalido(format!(
                    "no se puede determinar la carpeta padre de {}",
                    dest.display()
                ))
            })?;
            std::fs::create_dir_all(p)?;
            let temp = DirTemporal::new_in(p, ".obrain-ensayo-tmp-")?;
            Some((abs, temp))
        }
        None => None,
    };
    let scratch = match &parent_temp {
        Some(_) => None,
        None => Some(DirTemporal::new("obrain-ensayo-")?),
    };
    let work_dir = match (&parent_temp, &scratch) {
        (Some((_, t)), _) => t.path(),
        (None, Some(t)) => t.path(),
        _ => unreachable!(),
    };

    let ensayo_db = work_dir.join(BASE_ARCHIVO);
    let ensayo_llave = work_dir.join(LLAVE_ARCHIVO);
    copiar_sincronizado(&carpeta_respaldo.join(BASE_ARCHIVO), &ensayo_db)?;
    copiar_sincronizado(&carpeta_respaldo.join(LLAVE_ARCHIVO), &ensayo_llave)?;

    let recuperada_con_clave = if let Some(codigo) = codigo_recuperacion {
        seguridad::recuperar(&ensayo_db, codigo)?;
        true
    } else {
        false
    };

    let brain = crate::Brain::open(&ensayo_db).map_err(|e| match e {
        crate::Error::Seguridad(s) => Error::Seguridad(s),
        crate::Error::Core(c) => Error::Core(c),
        other => Error::InstantaneaNoCoincide(other.to_string()),
    })?;

    let estado_seg = brain.security().map_err(|e| match e {
        crate::Error::Seguridad(s) => Error::Seguridad(s),
        crate::Error::Core(c) => Error::Core(c),
        other => Error::InstantaneaNoCoincide(other.to_string()),
    })?;
    if !estado_seg.cifrada {
        return Err(Error::InstantaneaNoCoincide(
            "la copia de ensayo no quedó cifrada al abrirse".into(),
        ));
    }

    let pin_desbloqueado = if let Some(pin_str) = pin {
        brain.unlock(pin_str).map_err(|e| match e {
            crate::Error::Seguridad(s) => Error::Seguridad(s),
            crate::Error::Core(c) => Error::Core(c),
            other => Error::InstantaneaNoCoincide(other.to_string()),
        })?;
        true
    } else {
        false
    };

    let resumen_abierto = inspeccionar_instantanea(brain.store().conn())?;
    if resumen_abierto.entidades != manifiesto.instantanea.entidades
        || resumen_abierto.aserciones != manifiesto.instantanea.aserciones
        || resumen_abierto.fragmentos != manifiesto.instantanea.fragmentos
        || resumen_abierto.vectores != manifiesto.instantanea.vectores
        || resumen_abierto.huella_contenido != manifiesto.instantanea.huella_contenido
    {
        return Err(Error::InstantaneaNoCoincide(
            "los datos abiertos en el ensayo no coinciden con la instantánea del manifiesto".into(),
        ));
    }

    let bibliotecas_registradas =
        crate::indexar::libraries(brain.store()).map_err(|e| match e {
            crate::Error::Core(c) => Error::Core(c),
            other => Error::InstantaneaNoCoincide(other.to_string()),
        })?;
    let bibliotecas_ausentes_en_disco: Vec<String> = bibliotecas_registradas
        .iter()
        .filter(|ruta| !Path::new(ruta.as_str()).is_dir())
        .cloned()
        .collect();
    let bibliotecas_presentes = bibliotecas_registradas
        .len()
        .saturating_sub(bibliotecas_ausentes_en_disco.len());

    let documentos_en_boveda = brain
        .recent(usize::MAX)
        .map_err(|e| Error::InstantaneaNoCoincide(e.to_string()))?
        .len();
    let retenidos = brain
        .sensitive()
        .map_err(|e| Error::InstantaneaNoCoincide(e.to_string()))?;
    let sensibles = retenidos
        .iter()
        .filter(|r| r.sensitivity == "sensitive")
        .count();
    let boveda = retenidos
        .iter()
        .filter(|r| r.sensitivity == "vault")
        .count();
    let colecciones = brain
        .collections()
        .map_err(|e| Error::InstantaneaNoCoincide(e.to_string()))?
        .len();
    let vencimientos = brain
        .expiring()
        .map_err(|e| Error::InstantaneaNoCoincide(e.to_string()))?
        .len();
    let en_papelera = brain
        .trash()
        .map_err(|e| Error::InstantaneaNoCoincide(e.to_string()))?
        .len();

    drop(brain);

    let carpeta_ensayo = if let Some((dest_abs, staging_temp)) = parent_temp {
        publicar_staging(staging_temp, &dest_abs)?;
        Some(dest_abs)
    } else {
        None
    };

    Ok(InformeEnsayo {
        manifiesto,
        carpeta_ensayo,
        recuperada_con_clave,
        con_pin: estado_seg.pin,
        pin_desbloqueado,
        documentos_en_boveda,
        en_papelera,
        colecciones,
        vencimientos,
        sensibles,
        boveda,
        bibliotecas_registradas,
        bibliotecas_presentes,
        bibliotecas_ausentes_en_disco,
    })
}

/// Diagnóstico y plan detallado previo a la sustitución de un perfil existente.
pub fn planificar(
    carpeta_respaldo: &Path,
    db_destino: &Path,
    codigo_recuperacion: Option<&str>,
) -> Result<PlanRestauracion> {
    let manifiesto = verificar_con_clave(carpeta_respaldo, codigo_recuperacion)?;
    let db_destino_abs = absoluta(db_destino);
    let perfil_actual = inspeccionar_perfil(&db_destino_abs);

    let key = if let Some(c) = codigo_recuperacion {
        let parsed = RecoveryCode::parse(c)?;
        obrain_keys::unlock_recovery(&carpeta_respaldo.join(BASE_ARCHIVO), &parsed)?
    } else {
        obrain_keys::unlock(&carpeta_respaldo.join(BASE_ARCHIVO))?
    };
    let bibliotecas_respaldo_rutas = obtener_bibliotecas_respaldo(carpeta_respaldo, &key)?;
    let bibliotecas_respaldo: Vec<BibliotecaInfo> = bibliotecas_respaldo_rutas
        .into_iter()
        .map(|ruta| {
            let existe_en_disco = Path::new(&ruta).is_dir();
            BibliotecaInfo {
                ruta,
                existe_en_disco,
            }
        })
        .collect();

    let requiere_clave_recuperacion =
        obrain_keys::unlock(&carpeta_respaldo.join(BASE_ARCHIVO)).is_err();
    let llave_compatible_sistema = !requiere_clave_recuperacion;

    let mut advertencias = Vec::new();
    if perfil_actual.existe {
        advertencias.push(format!(
            "Se sustituirá el perfil existente en {} ({} entidades, {} aserciones).",
            db_destino.display(),
            perfil_actual.entidades,
            perfil_actual.aserciones
        ));
        if perfil_actual.entidades > manifiesto.instantanea.entidades
            || perfil_actual.aserciones > manifiesto.instantanea.aserciones
        {
            advertencias.push(
                "El perfil actual contiene más entidades o aserciones que el respaldo a restaurar (posible pérdida de novedades si no se respaldó previamente)."
                    .into(),
            );
        }
    } else {
        advertencias.push(format!(
            "No existe una base previa en {}; se configurará un nuevo perfil.",
            db_destino.display()
        ));
    }

    let ausentes_count = bibliotecas_respaldo
        .iter()
        .filter(|b| !b.existe_en_disco)
        .count();
    if ausentes_count > 0 {
        advertencias.push(format!(
            "{ausentes_count} carpeta(s) de biblioteca registradas no existen en este equipo. El índice recuperado se mantendrá protegido y la reindexación automática se suspenderá."
        ));
    }
    let presentes_count = bibliotecas_respaldo
        .iter()
        .filter(|b| b.existe_en_disco)
        .count();
    if presentes_count > 0 {
        advertencias.push(format!(
            "{presentes_count} carpeta(s) de biblioteca coinciden con rutas en este disco. La reindexación quedará en pausa hasta tu revisión."
        ));
    }
    if requiere_clave_recuperacion && codigo_recuperacion.is_none() {
        advertencias.push(
            "La llave del respaldo requiere la clave de recuperación para poder abrirse en este equipo."
                .into(),
        );
    }

    Ok(PlanRestauracion {
        manifiesto_respaldo: manifiesto,
        db_destino: db_destino_abs,
        perfil_actual,
        bibliotecas_respaldo,
        requiere_clave_recuperacion,
        llave_compatible_sistema,
        advertencias,
    })
}

/// Restaura de forma real, controlada y reversible un respaldo sobre `db_destino`.
pub fn restaurar(
    carpeta_respaldo: &Path,
    db_destino: &Path,
    opciones: OpcionesRestauracion,
) -> Result<InformeRestauracion> {
    restaurar_con_etapa(carpeta_respaldo, db_destino, opciones, |_| Ok(()))
}

/// Variante de [`restaurar`] con puntos de interrupción observables para comprobación
/// de errores y reversibilidad controlada en pruebas.
pub fn restaurar_con_etapa<F>(
    carpeta_respaldo: &Path,
    db_destino: &Path,
    opciones: OpcionesRestauracion,
    mut al_avanzar: F,
) -> Result<InformeRestauracion>
where
    F: FnMut(EtapaRestauracion) -> Result<()>,
{
    if !opciones.autorizar {
        return Err(Error::NoAutorizado);
    }

    let db_destino_abs = absoluta(db_destino);
    let parent = db_destino_abs.parent().ok_or_else(|| {
        Error::DestinoInvalido(format!(
            "no se puede determinar la carpeta padre de {}",
            db_destino.display()
        ))
    })?;
    std::fs::create_dir_all(parent)?;

    // 0. Si existía una transición anterior interrumpida, resolverla antes de proceder
    let _ = comprobar_y_reparar_transicion(&db_destino_abs)?;

    // 1. Verificar por completo el respaldo en modo solo lectura
    let manifiesto =
        verificar_con_clave(carpeta_respaldo, opciones.codigo_recuperacion.as_deref())?;
    al_avanzar(EtapaRestauracion::TrasVerificarRespaldo)?;

    // 2. Comprobar que no haya conexiones activas bloqueando la base
    comprobar_conexiones_activas(&db_destino_abs)?;
    let _perfil_actual = inspeccionar_perfil(&db_destino_abs);
    al_avanzar(EtapaRestauracion::TrasInspeccionarActiva)?;

    // 3. Crear salvaguarda recuperable del perfil actual si existe
    let salvaguarda_creada = if db_destino_abs.exists() {
        let s = crear_salvaguarda(&db_destino_abs, opciones.carpeta_salvaguarda.as_deref())?;
        Some(s)
    } else {
        None
    };
    al_avanzar(EtapaRestauracion::TrasCrearSalvaguarda)?;

    // 4. Preparar la nueva base y su material criptográfico en un staging aislado en el mismo volumen
    let staging = DirTemporal::new_in(parent, ".obrain-restaurar-staging-")?;
    let staged_db = staging.path().join(BASE_ARCHIVO);
    let staged_llave = staging.path().join(LLAVE_ARCHIVO);

    copiar_sincronizado(&carpeta_respaldo.join(BASE_ARCHIVO), &staged_db)?;
    copiar_sincronizado(&carpeta_respaldo.join(LLAVE_ARCHIVO), &staged_llave)?;

    let recuperada_con_clave = if let Some(ref codigo) = opciones.codigo_recuperacion {
        seguridad::recuperar(&staged_db, codigo)?;
        true
    } else {
        // Asegurar que abre con la caja del sistema actual (DPAPI)
        let _ = obrain_keys::unlock(&staged_db)?;
        false
    };

    let staged_key = obrain_keys::unlock(&staged_db)?;
    Store::verify_snapshot(&staged_db, &staged_key)?;

    let reindexado_en_pausa = if opciones.pausar_reindexado {
        let conn = Connection::open_with_flags(&staged_db, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        aplicar_llave_escritura(&conn, &staged_key)?;
        seguridad::pausar_reindexado(&conn, "restauración de respaldo reciente")?;
        Store::checkpoint(&conn)?;
        drop(conn);
        sincronizar_archivo(&staged_db)?;
        true
    } else {
        false
    };

    let staged_db_hash = describir_archivo(&staged_db, BASE_ARCHIVO)?.blake3;
    let staged_llave_hash = describir_archivo(&staged_llave, LLAVE_ARCHIVO)?.blake3;

    al_avanzar(EtapaRestauracion::TrasPrepararStaging)?;

    // 5. Persistir el recibo de transición antes de mover archivos activos
    let recibo_path = parent.join(RECIBO_TRANSICION_ARCHIVO);
    let target_llave = obrain_keys::key_path(&db_destino_abs);

    let (target_db_hash_anterior, target_llave_hash_anterior) = if db_destino_abs.exists() {
        let h_db = describir_archivo(&db_destino_abs, "db")?.blake3;
        let h_ll = if target_llave.exists() {
            Some(describir_archivo(&target_llave, "llave")?.blake3)
        } else {
            None
        };
        (Some(h_db), h_ll)
    } else {
        (None, None)
    };

    let mut recibo = ReciboTransicion {
        version: 1,
        creado_en_ms: ahora_ms(),
        salvaguarda_dir: salvaguarda_creada.clone(),
        staging_dir: staging.path().to_path_buf(),
        target_db: db_destino_abs.clone(),
        target_llave: target_llave.clone(),
        staged_db_hash: staged_db_hash.clone(),
        staged_llave_hash: staged_llave_hash.clone(),
        target_db_hash_anterior: target_db_hash_anterior.clone(),
        target_llave_hash_anterior,
        estado: "preparado".into(),
    };
    escribir_recibo_transicion(&recibo_path, &recibo)?;

    // 6. Sustitución controlada con reversibilidad
    let db_anterior = db_destino_abs.with_extension("obrain.anterior");
    let llave_anterior = target_llave.with_extension("llave.anterior");

    // Repetir comprobaciones críticas inmediatamente antes de la sustitución
    comprobar_conexiones_activas(&db_destino_abs)?;
    if db_destino_abs.exists() {
        if let Some(ref h_ant) = target_db_hash_anterior {
            let actual_hash = describir_archivo(&db_destino_abs, "db")?.blake3;
            if &actual_hash != h_ant {
                return Err(Error::DestinoInvalido(
                    "la base activa cambió durante la preparación de la restauración; operación cancelada para proteger los datos".into(),
                ));
            }
        }
    }

    let sustitucion_resultado = (|| -> Result<()> {
        if db_destino_abs.exists() {
            if db_anterior.exists() {
                let _ = std::fs::remove_file(&db_anterior);
            }
            std::fs::rename(&db_destino_abs, &db_anterior)?;
        }
        if target_llave.exists() {
            if llave_anterior.exists() {
                let _ = std::fs::remove_file(&llave_anterior);
            }
            std::fs::rename(&target_llave, &llave_anterior)?;
        }
        for suf in ["-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(con_sufijo(&db_destino_abs, suf));
        }

        recibo.estado = "reemplazando".into();
        escribir_recibo_transicion(&recibo_path, &recibo)?;
        al_avanzar(EtapaRestauracion::TrasDesplazarActiva)?;

        std::fs::rename(&staged_db, &db_destino_abs)?;
        std::fs::rename(&staged_llave, &target_llave)?;
        al_avanzar(EtapaRestauracion::TrasSustituirArchivos)?;

        al_avanzar(EtapaRestauracion::AntesDeValidarFinal)?;

        // 7. Validar el resultado final de la base instalada
        let brain_validado = crate::Brain::open(&db_destino_abs).map_err(|e| match e {
            crate::Error::Core(c) => Error::Core(c),
            crate::Error::Seguridad(s) => Error::Seguridad(s),
            crate::Error::Respaldo(r) => r,
            other => Error::InstantaneaNoCoincide(other.to_string()),
        })?;
        let resumen_final = inspeccionar_instantanea(brain_validado.store().conn())?;
        if resumen_final.entidades != manifiesto.instantanea.entidades
            || resumen_final.aserciones != manifiesto.instantanea.aserciones
            || resumen_final.huella_contenido != manifiesto.instantanea.huella_contenido
        {
            return Err(Error::InstantaneaNoCoincide(
                "los datos de la base restaurada no reproducen la huella del manifiesto".into(),
            ));
        }
        drop(brain_validado);

        Ok(())
    })();

    if let Err(e) = sustitucion_resultado {
        // Rollback automático: restablecer el perfil original
        let rollback_res = if db_anterior.exists() {
            revertir_sustitucion_anterior(
                &db_destino_abs,
                &target_llave,
                &db_anterior,
                &llave_anterior,
            )
        } else if let Some(ref salv) = salvaguarda_creada {
            revertir_sustitucion_salvaguarda(&db_destino_abs, &target_llave, salv)
        } else {
            Ok(())
        };
        let _ = std::fs::remove_file(&recibo_path);
        let _ = std::fs::remove_file(&db_anterior);
        let _ = std::fs::remove_file(&llave_anterior);

        return match rollback_res {
            Ok(()) => Err(Error::RestauracionFallidaYRecuperada(e.to_string())),
            Err(e_roll) => Err(Error::RestauracionCritica(
                e.to_string(),
                e_roll.to_string(),
                salvaguarda_creada.unwrap_or_else(|| parent.to_path_buf()),
            )),
        };
    }

    // Limpieza tras éxito completo
    let _ = std::fs::remove_file(&db_anterior);
    let _ = std::fs::remove_file(&llave_anterior);
    let _ = std::fs::remove_file(&recibo_path);
    let staged_folder = staging.keep();
    let _ = std::fs::remove_dir_all(&staged_folder);

    // Listar bibliotecas para informar
    let bibliotecas_respaldo_rutas = {
        let conn = Connection::open_with_flags(&db_destino_abs, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let k = obrain_keys::unlock(&db_destino_abs)?;
        aplicar_llave_lectura(&conn, &k)?;
        obtener_bibliotecas_de_conexion(&conn)?
    };
    let mut bibliotecas_presentes = Vec::new();
    let mut bibliotecas_ausentes = Vec::new();
    for ruta in bibliotecas_respaldo_rutas {
        if Path::new(&ruta).is_dir() {
            bibliotecas_presentes.push(ruta);
        } else {
            bibliotecas_ausentes.push(ruta);
        }
    }

    Ok(InformeRestauracion {
        manifiesto,
        db_destino: db_destino_abs,
        salvaguarda_creada,
        recuperada_con_clave,
        reindexado_en_pausa,
        bibliotecas_presentes,
        bibliotecas_ausentes,
    })
}

/// Comprueba si existe un recibo de transición pendiente por una restauración interrumpida,
/// y la repara automáticamente recuperando de forma fiable o finalizando la limpieza.
/// Ante cualquier ambigüedad, se detiene inmediatamente sin modificar archivos y preserva las evidencias.
pub fn comprobar_y_reparar_transicion(db_destino: &Path) -> Result<Option<String>> {
    let db_destino_abs = absoluta(db_destino);
    let parent = match db_destino_abs.parent() {
        Some(p) => p,
        None => return Ok(None),
    };
    let recibo_path = parent.join(RECIBO_TRANSICION_ARCHIVO);
    if !recibo_path.exists() {
        return Ok(None);
    }

    let texto = std::fs::read_to_string(&recibo_path)?;
    let recibo: ReciboTransicion = serde_json::from_str(&texto)
        .map_err(|e| Error::ManifiestoInvalido(format!("recibo de transición dañado: {e}")))?;

    let target_db = &recibo.target_db;
    let target_llave = &recibo.target_llave;

    let target_db_match = if target_db.exists() {
        describir_archivo(target_db, "db")
            .map(|m| m.blake3 == recibo.staged_db_hash)
            .unwrap_or(false)
    } else {
        false
    };
    let target_llave_match = if target_llave.exists() {
        describir_archivo(target_llave, "llave")
            .map(|m| m.blake3 == recibo.staged_llave_hash)
            .unwrap_or(false)
    } else {
        false
    };

    let db_anterior = target_db.with_extension("obrain.anterior");
    let llave_anterior = target_llave.with_extension("llave.anterior");

    // Caso 1: La sustitución completó satisfactoriamente (ambos archivos coinciden con el staging).
    if target_db_match && target_llave_match {
        let _ = std::fs::remove_file(&recibo_path);
        let _ = std::fs::remove_file(&db_anterior);
        let _ = std::fs::remove_file(&llave_anterior);
        let _ = std::fs::remove_dir_all(&recibo.staging_dir);
        return Ok(Some(
            "se completó y limpió una transición de restauración previa".to_string(),
        ));
    }

    // Caso 2: Interrupción antes de desplazar la base activa (el perfil previo está intacto en destino).
    let target_db_es_original = match &recibo.target_db_hash_anterior {
        Some(h_ant) => {
            target_db.exists()
                && describir_archivo(target_db, "db")
                    .map(|m| &m.blake3 == h_ant)
                    .unwrap_or(false)
        }
        None => !target_db.exists(),
    };
    if target_db_es_original && !db_anterior.exists() {
        let _ = std::fs::remove_file(&recibo_path);
        let _ = std::fs::remove_dir_all(&recibo.staging_dir);
        return Ok(Some(
            "se descartó una transición interrumpida antes de sustitución; el perfil original permaneció intacto".to_string(),
        ));
    }

    // Caso 3: Recuperable fiablemente desde db_anterior (.obrain.anterior).
    let db_anterior_valida = if db_anterior.exists() {
        match &recibo.target_db_hash_anterior {
            Some(h_ant) => describir_archivo(&db_anterior, "db")
                .map(|m| &m.blake3 == h_ant)
                .unwrap_or(false),
            None => true,
        }
    } else {
        false
    };

    if db_anterior_valida {
        revertir_sustitucion_anterior(target_db, target_llave, &db_anterior, &llave_anterior)?;
        let _ = std::fs::remove_file(&recibo_path);
        let _ = std::fs::remove_file(&db_anterior);
        let _ = std::fs::remove_file(&llave_anterior);
        let _ = std::fs::remove_dir_all(&recibo.staging_dir);
        return Ok(Some(
            "se recuperó el estado original desde los archivos previos tras detectar una restauración interrumpida".to_string(),
        ));
    }

    // Caso 4: Recuperable fiablemente desde la carpeta de salvaguarda.
    let salvaguarda_valida = if let Some(ref salv) = recibo.salvaguarda_dir {
        let src_db = salv.join(BASE_ARCHIVO);
        if src_db.exists() {
            match &recibo.target_db_hash_anterior {
                Some(h_ant) => describir_archivo(&src_db, "db")
                    .map(|m| &m.blake3 == h_ant)
                    .unwrap_or(false),
                None => true,
            }
        } else {
            false
        }
    } else {
        false
    };

    if salvaguarda_valida {
        let salv = recibo.salvaguarda_dir.as_ref().unwrap();
        revertir_sustitucion_salvaguarda(target_db, target_llave, salv)?;
        let _ = std::fs::remove_file(&recibo_path);
        let _ = std::fs::remove_file(&db_anterior);
        let _ = std::fs::remove_file(&llave_anterior);
        let _ = std::fs::remove_dir_all(&recibo.staging_dir);
        return Ok(Some(
            "se recuperó el estado original desde la salvaguarda tras detectar una restauración interrumpida".to_string(),
        ));
    }

    // Caso 5: Ambigüedad no recuperable de forma fiable:
    // NO modificar ni borrar ningún archivo. Preservar todas las evidencias y detenerse.
    Err(Error::TransicionAmbigua(recibo_path))
}

fn revertir_sustitucion_anterior(
    target_db: &Path,
    target_llave: &Path,
    db_anterior: &Path,
    llave_anterior: &Path,
) -> Result<()> {
    if target_db.exists() {
        let _ = std::fs::remove_file(target_db);
    }
    for suf in ["-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(con_sufijo(target_db, suf));
    }
    if target_llave.exists() {
        let _ = std::fs::remove_file(target_llave);
    }

    std::fs::rename(db_anterior, target_db)?;

    if llave_anterior.exists() {
        std::fs::rename(llave_anterior, target_llave)?;
    }

    Ok(())
}

fn revertir_sustitucion_salvaguarda(
    target_db: &Path,
    target_llave: &Path,
    salvaguarda_dir: &Path,
) -> Result<()> {
    if target_db.exists() {
        let _ = std::fs::remove_file(target_db);
    }
    for suf in ["-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(con_sufijo(target_db, suf));
    }
    if target_llave.exists() {
        let _ = std::fs::remove_file(target_llave);
    }

    let src_db = salvaguarda_dir.join(BASE_ARCHIVO);
    if src_db.exists() {
        copiar_sincronizado(&src_db, target_db)?;
    }

    let src_ll = salvaguarda_dir.join(LLAVE_ARCHIVO);
    if src_ll.exists() {
        copiar_sincronizado(&src_ll, target_llave)?;
    }

    Ok(())
}

fn escribir_recibo_transicion(path: &Path, recibo: &ReciboTransicion) -> Result<()> {
    let json = serde_json::to_string_pretty(recibo)
        .map_err(|e| Error::ManifiestoInvalido(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn crear_salvaguarda(db: &Path, destino_personalizado: Option<&Path>) -> Result<PathBuf> {
    let parent = db.parent().ok_or_else(|| {
        Error::DestinoInvalido(format!(
            "no se puede determinar la carpeta padre de {}",
            db.display()
        ))
    })?;

    let salvaguarda_dir = match destino_personalizado {
        Some(p) => {
            let p_abs = absoluta(p);
            std::fs::create_dir_all(&p_abs)?;
            p_abs
        }
        None => {
            let dir_base = parent.join("salvaguardas");
            std::fs::create_dir_all(&dir_base)?;
            let mut azar = [0u8; 4];
            let _ = getrandom::fill(&mut azar);
            let hex: String = azar.iter().map(|b| format!("{b:02x}")).collect();
            let d = dir_base.join(format!("salvaguarda-{}-{hex}", ahora_ms()));
            std::fs::create_dir_all(&d)?;
            d
        }
    };

    let target_db = salvaguarda_dir.join(BASE_ARCHIVO);
    let target_llave = salvaguarda_dir.join(LLAVE_ARCHIVO);

    copiar_sincronizado(db, &target_db)?;
    let src_llave = obrain_keys::key_path(db);
    if src_llave.exists() {
        copiar_sincronizado(&src_llave, &target_llave)?;
    }

    for suf in ["-wal", "-shm"] {
        let src_aux = con_sufijo(db, suf);
        if src_aux.exists() && src_aux.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            let dst_aux = con_sufijo(&target_db, suf);
            let _ = copiar_sincronizado(&src_aux, &dst_aux);
        }
    }

    let meta_db = describir_archivo(&target_db, BASE_ARCHIVO)?;
    let meta_llave = if target_llave.exists() {
        Some(describir_archivo(&target_llave, LLAVE_ARCHIVO)?)
    } else {
        None
    };

    #[derive(serde::Serialize)]
    struct InfoSalvaguarda {
        creado_en_ms: i64,
        db_original: String,
        base: ArchivoRespaldo,
        llave: Option<ArchivoRespaldo>,
    }

    let info = InfoSalvaguarda {
        creado_en_ms: ahora_ms(),
        db_original: db.display().to_string(),
        base: meta_db,
        llave: meta_llave,
    };

    let info_path = salvaguarda_dir.join("info.json");
    let json = serde_json::to_string_pretty(&info)
        .map_err(|e| Error::ManifiestoInvalido(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&info_path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;

    Ok(salvaguarda_dir)
}

fn inspeccionar_perfil(db: &Path) -> ResumenPerfil {
    if !db.exists() {
        return ResumenPerfil {
            existe: false,
            cifrada: false,
            abrible: false,
            esquema_version: 0,
            entidades: 0,
            aserciones: 0,
            aserciones_vigentes: 0,
            fragmentos: 0,
            vectores: 0,
            tiene_pin: false,
            huella_contenido: None,
            bibliotecas: Vec::new(),
            error_acceso: None,
        };
    }

    let cifrada = obrain_keys::exists(db);
    let key_opt = if cifrada {
        match obrain_keys::unlock(db) {
            Ok(k) => Some(k),
            Err(e) => {
                return ResumenPerfil {
                    existe: true,
                    cifrada: true,
                    abrible: false,
                    esquema_version: 0,
                    entidades: 0,
                    aserciones: 0,
                    aserciones_vigentes: 0,
                    fragmentos: 0,
                    vectores: 0,
                    tiene_pin: false,
                    huella_contenido: None,
                    bibliotecas: Vec::new(),
                    error_acceso: Some(e.to_string()),
                };
            }
        }
    } else {
        None
    };

    let inspect_res = (|| -> Result<(ResumenInstantanea, Vec<String>)> {
        let private = DirTemporal::new("obrain-inspect-perfil-")?;
        let copy_db = private.path().join(BASE_ARCHIVO);
        std::fs::copy(db, &copy_db)?;
        let conn = Connection::open_with_flags(&copy_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if let Some(ref k) = key_opt {
            aplicar_llave_lectura(&conn, k)?;
        }
        let resumen = inspeccionar_instantanea(&conn)?;
        let bibs = obtener_bibliotecas_de_conexion(&conn)?;
        Ok((resumen, bibs))
    })();

    match inspect_res {
        Ok((resumen, bibliotecas)) => ResumenPerfil {
            existe: true,
            cifrada,
            abrible: true,
            esquema_version: resumen.esquema_version,
            entidades: resumen.entidades,
            aserciones: resumen.aserciones,
            aserciones_vigentes: resumen.aserciones_vigentes,
            fragmentos: resumen.fragmentos,
            vectores: resumen.vectores,
            tiene_pin: resumen.tiene_pin,
            huella_contenido: Some(resumen.huella_contenido),
            bibliotecas,
            error_acceso: None,
        },
        Err(e) => ResumenPerfil {
            existe: true,
            cifrada,
            abrible: false,
            esquema_version: 0,
            entidades: 0,
            aserciones: 0,
            aserciones_vigentes: 0,
            fragmentos: 0,
            vectores: 0,
            tiene_pin: false,
            huella_contenido: None,
            bibliotecas: Vec::new(),
            error_acceso: Some(e.to_string()),
        },
    }
}

fn comprobar_conexiones_activas(db: &Path) -> Result<()> {
    if !db.exists() {
        return Ok(());
    }

    // 1. Checkpoint con timeout en SQLite
    let conn_res = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_WRITE);
    match conn_res {
        Ok(conn) => {
            let _ = conn.busy_timeout(std::time::Duration::from_millis(500));
            if obrain_keys::exists(db) {
                if let Ok(k) = obrain_keys::unlock(db) {
                    let _ = conn.execute_batch(&format!(
                        "PRAGMA key = \"{}\"; PRAGMA temp_store = MEMORY;",
                        k.sqlcipher_literal().as_str()
                    ));
                }
            }
            if conn
                .execute_batch("PRAGMA locking_mode = EXCLUSIVE;")
                .is_err()
            {
                return Err(Error::BaseEnUso(db.to_path_buf()));
            }
            let check: std::result::Result<(i64, i64, i64), _> =
                conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                });
            match check {
                Ok((busy, frames, copied)) => {
                    if busy != 0 || (frames >= 0 && frames != copied) {
                        return Err(Error::BaseEnUso(db.to_path_buf()));
                    }
                }
                Err(_) => {
                    return Err(Error::BaseEnUso(db.to_path_buf()));
                }
            }
            drop(conn);
        }
        Err(_) => {
            return Err(Error::BaseEnUso(db.to_path_buf()));
        }
    }

    // 2. Comprobar que el archivo se pueda abrir con exclusividad en el SO
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        for path in [db.to_path_buf(), con_sufijo(db, "-wal")] {
            if path.exists() {
                match std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(&path)
                {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        return Err(Error::BaseEnUso(db.to_path_buf()));
                    }
                    Err(e) => return Err(Error::Io(e)),
                }
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        match std::fs::OpenOptions::new().read(true).write(true).open(db) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(Error::BaseEnUso(db.to_path_buf()))
            }
            Err(e) => Err(Error::Io(e)),
        }
    }
}

fn obtener_bibliotecas_de_conexion(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a_path.object_text \
         FROM assertions a_facet \
         JOIN assertions a_path ON a_facet.subject = a_path.subject \
         WHERE a_facet.predicate = 'has_facet' \
           AND a_facet.object_text = 'library' \
           AND a_facet.retracted_at IS NULL \
           AND a_path.predicate = 'folder_path' \
           AND a_path.retracted_at IS NULL \
         ORDER BY a_path.object_text",
    )?;
    let mut rows = stmt.query([])?;
    let mut bibliotecas = Vec::new();
    while let Some(row) = rows.next()? {
        if let Some(t) = row.get::<_, Option<String>>(0)? {
            bibliotecas.push(t);
        }
    }
    Ok(bibliotecas)
}

fn obtener_bibliotecas_respaldo(
    carpeta_respaldo: &Path,
    key: &obrain_keys::DataKey,
) -> Result<Vec<String>> {
    let base_path = carpeta_respaldo.join(BASE_ARCHIVO);
    let private = DirTemporal::new("obrain-inspect-bibs-")?;
    let copy_db = private.path().join(BASE_ARCHIVO);
    std::fs::copy(&base_path, &copy_db)?;
    let conn = Connection::open_with_flags(&copy_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    aplicar_llave_lectura(&conn, key)?;
    obtener_bibliotecas_de_conexion(&conn)
}

fn aplicar_llave_escritura(conn: &Connection, key: &obrain_keys::DataKey) -> Result<()> {
    conn.execute_batch(&format!(
        "PRAGMA key = \"{}\"; PRAGMA temp_store = MEMORY;",
        key.sqlcipher_literal().as_str()
    ))?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|_| Error::LlaveNoCorresponde("la llave no abre la base staged".into()))?;
    Ok(())
}

fn validar_origen(db: &Path) -> Result<()> {
    if !db.is_file() || db.metadata()?.len() == 0 {
        return Err(Error::Core(obrain_core::Error::UnrecognizedDatabase(
            "falta el archivo de base o está vacío".into(),
        )));
    }
    for sufijo in [
        ".cifrando",
        ".cifrando-wal",
        ".cifrando-shm",
        ".cifrando-journal",
        ".llave.tmp",
    ] {
        let mut s = db.as_os_str().to_owned();
        s.push(sufijo);
        if PathBuf::from(s).exists() {
            return Err(Error::OrigenConRestos(db.to_path_buf()));
        }
    }
    if !obrain_keys::exists(db) {
        return Err(Error::OrigenSinCifrar(db.to_path_buf()));
    }
    Ok(())
}

fn validar_destino_nuevo(db: &Path, destino: &Path) -> Result<()> {
    if let Some(nombre) = destino.file_name().and_then(|n| n.to_str()) {
        if nombre.starts_with(".obrain-respaldo-tmp-") || nombre.starts_with(".obrain-ensayo-tmp-")
        {
            return Err(Error::DestinoInvalido(
                "el nombre de la carpeta de respaldo no puede utilizar el prefijo reservado de directorios temporales".into(),
            ));
        }
    }
    let db_abs = absoluta(db);
    let dest_abs = absoluta(destino);
    if dest_abs == db_abs
        || dest_abs.starts_with(&db_abs)
        || db_abs.parent().is_some_and(|p| p == dest_abs)
    {
        return Err(Error::DestinoInvalido(
            "la carpeta de respaldo debe ser independiente de la carpeta de la base activa".into(),
        ));
    }
    comprobar_carpeta_vacia_o_inexistente(destino)
}

fn validar_destino_ensayo(carpeta_respaldo: &Path, destino_ensayo: &Path) -> Result<()> {
    if let Some(nombre) = destino_ensayo.file_name().and_then(|n| n.to_str()) {
        if nombre.starts_with(".obrain-respaldo-tmp-") || nombre.starts_with(".obrain-ensayo-tmp-")
        {
            return Err(Error::DestinoInvalido(
                "el nombre de la carpeta de ensayo no puede utilizar el prefijo reservado de directorios temporales".into(),
            ));
        }
    }
    let respaldo_abs = absoluta(carpeta_respaldo);
    let ensayo_abs = absoluta(destino_ensayo);
    if ensayo_abs == respaldo_abs || ensayo_abs.starts_with(&respaldo_abs) {
        return Err(Error::DestinoInvalido(
            "la carpeta de ensayo no puede ser la misma carpeta del respaldo".into(),
        ));
    }
    let activa_abs = absoluta(&crate::default_db_path());
    if ensayo_abs == activa_abs
        || activa_abs.parent().is_some_and(|p| p == ensayo_abs)
        || activa_abs.starts_with(&ensayo_abs)
    {
        return Err(Error::DestinoInvalido(
            "la carpeta de ensayo no puede coincidir con la ubicación de la base activa".into(),
        ));
    }
    comprobar_carpeta_vacia_o_inexistente(destino_ensayo)
}

fn comprobar_carpeta_vacia_o_inexistente(destino: &Path) -> Result<()> {
    if destino.exists() {
        if !destino.is_dir() {
            return Err(Error::DestinoOcupado(destino.to_path_buf()));
        }
        if std::fs::read_dir(destino)?.next().is_some() {
            return Err(Error::DestinoOcupado(destino.to_path_buf()));
        }
    }
    Ok(())
}

struct DirTemporal {
    path: PathBuf,
    activo: bool,
}

impl DirTemporal {
    fn new_in(parent: &Path, prefix: &str) -> std::io::Result<Self> {
        for _ in 0..64 {
            let mut rnd = [0u8; 12];
            let _ = getrandom::fill(&mut rnd);
            let mut hex = String::with_capacity(24);
            for b in rnd {
                use std::fmt::Write as _;
                let _ = write!(&mut hex, "{b:02x}");
            }
            let candidate = parent.join(format!("{prefix}{hex}"));
            match std::fs::create_dir(&candidate) {
                Ok(()) => {
                    return Ok(Self {
                        path: candidate,
                        activo: true,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "no se pudo crear un directorio temporal único",
        ))
    }

    fn new(prefix: &str) -> std::io::Result<Self> {
        Self::new_in(&std::env::temp_dir(), prefix)
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn keep(mut self) -> PathBuf {
        self.activo = false;
        std::mem::take(&mut self.path)
    }
}

impl Drop for DirTemporal {
    fn drop(&mut self) {
        if self.activo {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn publicar_staging(staging: DirTemporal, destino_abs: &Path) -> Result<()> {
    if destino_abs.exists() {
        if !destino_abs.is_dir() || std::fs::read_dir(destino_abs)?.next().is_some() {
            return Err(Error::DestinoOcupado(destino_abs.to_path_buf()));
        }
        std::fs::remove_dir(destino_abs)?;
    }
    let staged_path = staging.keep();
    if let Err(err) = std::fs::rename(&staged_path, destino_abs) {
        let _ = std::fs::remove_dir_all(&staged_path);
        return Err(err.into());
    }
    Ok(())
}

fn aplicar_llave_lectura(conn: &Connection, key: &obrain_keys::DataKey) -> Result<()> {
    conn.execute_batch(&format!(
        "PRAGMA key = \"{}\"; PRAGMA temp_store = MEMORY;",
        key.sqlcipher_literal().as_str()
    ))?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|_| {
        Error::LlaveNoCorresponde("la llave no abre la instantánea brain.obrain".into())
    })?;
    Ok(())
}

fn tabla_existe(conn: &Connection, nombre: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [nombre],
        |r| r.get(0),
    )?)
}

fn inspeccionar_instantanea(conn: &Connection) -> Result<ResumenInstantanea> {
    conn.execute_batch("BEGIN DEFERRED")?;
    let res = inspeccionar_instantanea_inner(conn);
    let _ = conn.execute_batch("COMMIT");
    res
}

fn inspeccionar_instantanea_inner(conn: &Connection) -> Result<ResumenInstantanea> {
    let esquema_version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let entidades: i64 = conn.query_row("SELECT count(*) FROM entities", [], |r| r.get(0))?;
    let aserciones: i64 = conn.query_row("SELECT count(*) FROM assertions", [], |r| r.get(0))?;
    let aserciones_vigentes: i64 = conn.query_row(
        "SELECT count(*) FROM assertions WHERE retracted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let predicados: i64 = conn.query_row("SELECT count(*) FROM predicates", [], |r| r.get(0))?;

    let tiene_chunks = tabla_existe(conn, "chunks")?;
    let fragmentos: i64 = if tiene_chunks {
        conn.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0))?
    } else {
        0
    };

    let tiene_vectors = tabla_existe(conn, "vectors")?;
    let vectores: i64 = if tiene_vectors {
        conn.query_row("SELECT count(*) FROM vectors", [], |r| r.get(0))?
    } else {
        0
    };

    let tiene_publicacion = tabla_existe(conn, "content_publication")?;
    let (publicaciones_listas, publicaciones_pendientes) = if tiene_publicacion {
        let listas: i64 = conn.query_row(
            "SELECT count(*) FROM content_publication WHERE state = 'ready'",
            [],
            |r| r.get(0),
        )?;
        let pendientes: i64 = conn.query_row(
            "SELECT count(*) FROM content_publication WHERE state = 'pending'",
            [],
            |r| r.get(0),
        )?;
        (listas, pendientes)
    } else {
        (0, 0)
    };

    let tiene_seguridad = tabla_existe(conn, "seguridad")?;
    let (tiene_pin, tiene_recuperacion_interna) = if tiene_seguridad {
        let pin: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM seguridad WHERE clave = 'pin')",
            [],
            |r| r.get(0),
        )?;
        let rec: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM seguridad WHERE clave = 'recuperacion')",
            [],
            |r| r.get(0),
        )?;
        (pin, rec)
    } else {
        (false, false)
    };

    let huella_contenido = calcular_huella_contenido(
        conn,
        esquema_version,
        tiene_publicacion,
        tiene_chunks,
        tiene_vectors,
    )?;

    Ok(ResumenInstantanea {
        esquema_version,
        entidades,
        aserciones,
        aserciones_vigentes,
        predicados,
        fragmentos,
        vectores,
        publicaciones_listas,
        publicaciones_pendientes,
        tiene_pin,
        tiene_recuperacion_interna,
        huella_contenido,
    })
}

fn alimentar_str(hasher: &mut blake3::Hasher, s: &str) {
    hasher.update(&(s.len() as u64).to_le_bytes());
    hasher.update(s.as_bytes());
}

fn alimentar_opt_str(hasher: &mut blake3::Hasher, opt: Option<&str>) {
    match opt {
        Some(s) => {
            hasher.update(&[1]);
            alimentar_str(hasher, s);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn alimentar_opt_i64(hasher: &mut blake3::Hasher, opt: Option<i64>) {
    match opt {
        Some(v) => {
            hasher.update(&[1]);
            hasher.update(&v.to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn alimentar_opt_f64(hasher: &mut blake3::Hasher, opt: Option<f64>) {
    match opt {
        Some(v) => {
            hasher.update(&[1]);
            hasher.update(&v.to_bits().to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn calcular_huella_contenido(
    conn: &Connection,
    esquema_version: i32,
    tiene_publicacion: bool,
    tiene_chunks: bool,
    tiene_vectors: bool,
) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    alimentar_str(&mut hasher, "obrain-instantanea-v1");
    hasher.update(&esquema_version.to_le_bytes());

    alimentar_str(&mut hasher, "entities");
    {
        let mut stmt =
            conn.prepare("SELECT id, created_at, sensitivity FROM entities ORDER BY id")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let created_at: i64 = row.get(1)?;
            let sensitivity: String = row.get(2)?;
            alimentar_str(&mut hasher, &id);
            hasher.update(&created_at.to_le_bytes());
            alimentar_str(&mut hasher, &sensitivity);
        }
    }

    alimentar_str(&mut hasher, "assertions");
    {
        let mut stmt = conn.prepare(
            "SELECT id, subject, predicate, object_kind, object_entity, object_text,
                    object_number, valid_from, valid_to, asserted_at, retracted_at,
                    method, source, confidence, evidence
               FROM assertions
              ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let subject: String = row.get(1)?;
            let predicate: String = row.get(2)?;
            let object_kind: String = row.get(3)?;
            let object_entity: Option<String> = row.get(4)?;
            let object_text: Option<String> = row.get(5)?;
            let object_number: Option<f64> = row.get(6)?;
            let valid_from: Option<i64> = row.get(7)?;
            let valid_to: Option<i64> = row.get(8)?;
            let asserted_at: i64 = row.get(9)?;
            let retracted_at: Option<i64> = row.get(10)?;
            let method: String = row.get(11)?;
            let source: String = row.get(12)?;
            let confidence: f64 = row.get(13)?;
            let evidence: Option<String> = row.get(14)?;

            alimentar_str(&mut hasher, &id);
            alimentar_str(&mut hasher, &subject);
            alimentar_str(&mut hasher, &predicate);
            alimentar_str(&mut hasher, &object_kind);
            alimentar_opt_str(&mut hasher, object_entity.as_deref());
            alimentar_opt_str(&mut hasher, object_text.as_deref());
            alimentar_opt_f64(&mut hasher, object_number);
            alimentar_opt_i64(&mut hasher, valid_from);
            alimentar_opt_i64(&mut hasher, valid_to);
            hasher.update(&asserted_at.to_le_bytes());
            alimentar_opt_i64(&mut hasher, retracted_at);
            alimentar_str(&mut hasher, &method);
            alimentar_str(&mut hasher, &source);
            hasher.update(&confidence.to_bits().to_le_bytes());
            alimentar_opt_str(&mut hasher, evidence.as_deref());
        }
    }

    if tiene_publicacion {
        alimentar_str(&mut hasher, "content_publication");
        let mut stmt = conn.prepare(
            "SELECT entity, state, origin, observed_hash, published_hash, backup_captured
               FROM content_publication
              ORDER BY entity",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let entity: String = row.get(0)?;
            let state: String = row.get(1)?;
            let origin: String = row.get(2)?;
            let observed_hash: Option<String> = row.get(3)?;
            let published_hash: Option<String> = row.get(4)?;
            let backup_captured: i64 = row.get(5)?;
            alimentar_str(&mut hasher, &entity);
            alimentar_str(&mut hasher, &state);
            alimentar_str(&mut hasher, &origin);
            alimentar_opt_str(&mut hasher, observed_hash.as_deref());
            alimentar_opt_str(&mut hasher, published_hash.as_deref());
            hasher.update(&backup_captured.to_le_bytes());
        }
    }

    if tiene_chunks {
        alimentar_str(&mut hasher, "chunks");
        let mut stmt = conn.prepare(
            "SELECT entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text
               FROM chunks
              ORDER BY entity, chunker, chunker_ver, ordinal",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let entity: String = row.get(0)?;
            let chunker: String = row.get(1)?;
            let chunker_ver: i64 = row.get(2)?;
            let ordinal: i64 = row.get(3)?;
            let byte_start: i64 = row.get(4)?;
            let byte_end: i64 = row.get(5)?;
            let text: String = row.get(6)?;
            alimentar_str(&mut hasher, &entity);
            alimentar_str(&mut hasher, &chunker);
            hasher.update(&chunker_ver.to_le_bytes());
            hasher.update(&ordinal.to_le_bytes());
            hasher.update(&byte_start.to_le_bytes());
            hasher.update(&byte_end.to_le_bytes());
            alimentar_str(&mut hasher, &text);
        }
    }

    if tiene_vectors {
        alimentar_str(&mut hasher, "vectors");
        let mut stmt = conn.prepare(
            "SELECT entity, chunk, model_id, model_ver, dim, chunker, chunker_ver, vec
               FROM vectors
              ORDER BY entity, chunk, model_id, model_ver, chunker, chunker_ver",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let entity: String = row.get(0)?;
            let chunk: i64 = row.get(1)?;
            let model_id: String = row.get(2)?;
            let model_ver: i64 = row.get(3)?;
            let dim: i64 = row.get(4)?;
            let chunker: String = row.get(5)?;
            let chunker_ver: i64 = row.get(6)?;
            let vec: Vec<u8> = row.get(7)?;
            alimentar_str(&mut hasher, &entity);
            hasher.update(&chunk.to_le_bytes());
            alimentar_str(&mut hasher, &model_id);
            hasher.update(&model_ver.to_le_bytes());
            hasher.update(&dim.to_le_bytes());
            alimentar_str(&mut hasher, &chunker);
            hasher.update(&chunker_ver.to_le_bytes());
            hasher.update(&(vec.len() as u64).to_le_bytes());
            hasher.update(&vec);
        }
    }

    Ok(hasher.finalize().to_hex().to_string())
}

fn describir_diferencia(esperado: &ResumenInstantanea, actual: &ResumenInstantanea) -> String {
    if esperado.esquema_version != actual.esquema_version {
        return format!(
            "versión de esquema {} != {}",
            esperado.esquema_version, actual.esquema_version
        );
    }
    if esperado.entidades != actual.entidades {
        return format!("entidades {} != {}", esperado.entidades, actual.entidades);
    }
    if esperado.aserciones != actual.aserciones {
        return format!(
            "aserciones {} != {}",
            esperado.aserciones, actual.aserciones
        );
    }
    if esperado.aserciones_vigentes != actual.aserciones_vigentes {
        return format!(
            "aserciones vigentes {} != {}",
            esperado.aserciones_vigentes, actual.aserciones_vigentes
        );
    }
    if esperado.predicados != actual.predicados {
        return format!(
            "predicados {} != {}",
            esperado.predicados, actual.predicados
        );
    }
    if esperado.fragmentos != actual.fragmentos {
        return format!(
            "fragmentos {} != {}",
            esperado.fragmentos, actual.fragmentos
        );
    }
    if esperado.vectores != actual.vectores {
        return format!("vectores {} != {}", esperado.vectores, actual.vectores);
    }
    if esperado.publicaciones_listas != actual.publicaciones_listas
        || esperado.publicaciones_pendientes != actual.publicaciones_pendientes
    {
        return format!(
            "estado de publicación ({}/{}) != ({}/{})",
            esperado.publicaciones_listas,
            esperado.publicaciones_pendientes,
            actual.publicaciones_listas,
            actual.publicaciones_pendientes
        );
    }
    if esperado.tiene_pin != actual.tiene_pin
        || esperado.tiene_recuperacion_interna != actual.tiene_recuperacion_interna
    {
        return "ajustes de seguridad distintos a los del manifiesto".into();
    }
    "la huella criptográfica del contenido no coincide con el manifiesto".into()
}

fn describir_archivo(path: &Path, nombre: &str) -> Result<ArchivoRespaldo> {
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Err(Error::ArchivoVacio(path.to_path_buf()));
    }
    Ok(ArchivoRespaldo {
        nombre: nombre.to_string(),
        bytes: bytes.len() as u64,
        blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

fn comprobar_archivo(esperado: &ArchivoRespaldo, actual: &ArchivoRespaldo) -> Result<()> {
    if esperado.bytes != actual.bytes {
        return Err(Error::TamanoNoCoincide {
            archivo: esperado.nombre.clone(),
            esperado: esperado.bytes,
            actual: actual.bytes,
        });
    }
    if esperado.blake3 != actual.blake3 {
        return Err(Error::HashNoCoincide {
            archivo: esperado.nombre.clone(),
            esperado: esperado.blake3.clone(),
            actual: actual.blake3.clone(),
        });
    }
    Ok(())
}

fn copiar_sincronizado(origen: &Path, destino: &Path) -> Result<()> {
    let bytes = std::fs::read(origen)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destino)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn sincronizar_archivo(path: &Path) -> Result<()> {
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.sync_all()?;
    Ok(())
}

fn escribir_manifiesto(path: &Path, manifiesto: &Manifiesto) -> Result<()> {
    let json = serde_json::to_string_pretty(manifiesto)
        .map_err(|e| Error::ManifiestoInvalido(e.to_string()))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn absoluta(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

fn con_sufijo(p: &Path, sufijo: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(sufijo);
    PathBuf::from(s)
}

fn ahora_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
