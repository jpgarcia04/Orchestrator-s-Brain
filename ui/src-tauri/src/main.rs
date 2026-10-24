//! La aplicación de escritorio. **Unos pocos comandos y ni una decisión.**
//!
//! Todo lo que hay aquí es transporte: coger el cerrojo, llamar a
//! [`obrain_api`] y aplastar el error a texto. Si algún comando tuviera lógica
//! propia —qué es relevante, qué es un hecho, cómo se llama un nodo— sería
//! lógica encerrada dentro de una aplicación de escritorio, y el contestador de
//! M7 tendría que volver a escribirla.
//!
//! ## Las dos salidas hacia el archivo real, y por qué van por aquí
//!
//! [`abrir`] y [`revelar`] son lo único que esta aplicación hace **fuera de su
//! propia ventana**, y las dos reciben un **identificador de entidad, nunca una
//! ruta**. La diferencia no es de estilo: un comando que aceptara una ruta
//! sería un «abre esto» genérico expuesto al webview, y la aplicación pasaría
//! de poder tocar lo que está indexado a poder tocar cualquier cosa del disco.
//! Por eso tampoco se le dan al webview los permisos del plugin: la decisión de
//! qué se puede abrir se toma en Rust, contra el log. Y con PIN (ADR-0021), lo
//! sensible sólo se abre desbloqueado: abrirlo fuera sería saltarse el PIN.
//!
//! ## El modelo se carga en un hilo aparte, y es lo que decide la sensación
//!
//! `EXPERIENCE.md` §2: *«abrir en 40 ms se siente caro; abrir en 400 ms se
//! siente barato aunque tenga shaders»*. Cargar `multilingual-e5-small` cuesta
//! **2,50 s**. Así que la ventana abre con la base ya lista y la búsqueda
//! léxica funcionando —0,5–1,9 ms por consulta, informe de M2— mientras un hilo
//! de fondo carga el modelo y lo instala cuando esté. Los 2,5 s existen; lo que
//! no existe es una pantalla parada mirándolos.
//!
//! ## Tus bibliotecas se ponen al día solas
//!
//! Al arrancar, y cada vez que se pulsa «actualizar», se vuelve a indexar cada
//! biblioteca registrada ([`obrain_api::indexar`]). Va en un hilo y **con su
//! propia conexión** a la base: la ventana sigue leyendo mientras tanto (la
//! base está en modo WAL), y el modelo se presta al indexador en vez de cargarse
//! dos veces. Al terminar se avisa a la ventana con el evento `indexado`.
//!
//! Y mientras está abierta, **un vigía** (ADR-0024) mira tus bibliotecas:
//! cuando algo cambia y lleva tres segundos quieto, lanza esa misma pasada
//! —sin aviso: sólo el indicador de abajo a la izquierda lo dice—. Nunca dos a
//! la vez: si algo cambia a media pasada, al terminar sale una más.
//!
//! ## La base está cifrada, y puede estar cerrada
//!
//! Al abrir, la base se cifra si venía en claro (ADR-0021) y se abre con la
//! llave que guarda Windows. Si este equipo no tiene esa llave —otro equipo,
//! Windows reinstalado—, la aplicación **arranca igual, con la base cerrada**, y
//! la ventana pide la clave de recuperación ([`recuperar`]). Todo lo demás
//! responde «la base está cerrada» hasta entonces.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use obrain_api::respaldo::{
    self, InformeEnsayo, InformeRestauracion, Manifiesto, OpcionesRestauracion, PlanRestauracion,
};
use obrain_api::seguridad::{self, Estado as EstadoSeguridad};
use obrain_api::vigilar::{self, Vigia};
use obrain_api::{
    Abierta, Brain, Coleccion, Embedder, EnPapelera, EntityView, Expiring, FolderMarked, Glance,
    IndexReport, Lens, Listing, MarkedFolder, Preview, Row, Sky, default_db_path,
};
use obrain_core::{EntityId, Sensitivity, Store};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// El cerebro, o `None` si la base está cerrada (falta su llave).
struct Cerebro(Mutex<Option<Brain>>);

/// ¿Hay un reindexado en marcha? Dos a la vez escribirían lo mismo dos veces.
struct Indexando(AtomicBool);

/// El reconocedor de texto (OCR) de este equipo, si lo hay. Se mira una vez.
struct MotorOcr(Option<String>);

/// El vigía de las bibliotecas (ADR-0024), o `None` si el sistema no deja
/// vigilar: entonces todo sigue como antes, al abrir y con «actualizar».
struct Vigilante(Mutex<Option<Vigia>>);

/// Algo cambió con una pasada en marcha: al terminar, sale otra.
struct Pendiente(AtomicBool);

/// Estado seguro de operaciones de respaldo y restauración.
/// Retiene la carpeta seleccionada por el usuario desde el diálogo de Windows
/// y un cerrojo atómico para impedir restauraciones concurrentes.
struct EstadoRespaldo {
    seleccionada: Mutex<Option<PathBuf>>,
    restaurando: AtomicBool,
}

#[derive(serde::Serialize)]
struct RespaldoInfoResumen {
    carpeta: String,
    nombre_carpeta: String,
    manifiesto: Manifiesto,
    requiere_clave: bool,
}

#[derive(serde::Serialize)]
struct ResultadoCrearRespaldo {
    carpeta: String,
    nombre_carpeta: String,
    manifiesto: Manifiesto,
}

const CERRADA: &str = "la base está cerrada: escribe tu clave de recuperación";

/// Lo que la barra enseña al abrir, antes de que nadie escriba nada.
#[derive(serde::Serialize)]
struct Estado {
    db: String,
    /// Windows no tiene la llave de la base: hace falta la clave de
    /// recuperación. Con la base cerrada, el resto de campos van vacíos.
    cerrada: bool,
    semantic: bool,
    entities: i64,
    assertions: i64,
    /// Las carpetas que el usuario eligió indexar. Vacío = bóveda sin empezar.
    libraries: Vec<String>,
    /// Hay un reindexado en curso.
    indexing: bool,
    /// El OCR de este equipo (`windows.media.ocr/es-ES`), o `None` si no hay:
    /// entonces las fotos y los PDF escaneados no tienen texto que buscar.
    ocr: Option<String>,
}

/// Lo que recibe la ventana con el evento `indexado`.
#[derive(serde::Serialize, Clone)]
struct Indexado {
    report: Option<IndexReport>,
    error: Option<String>,
    /// La lanzó el vigía, no tú: la ventana no avisa de lo nuevo (ADR-0024).
    live: bool,
}

fn texto<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// El único acceso al cerebro desde los comandos: con el cerrojo, y con la
/// base abierta.
fn con<T>(
    cerebro: &State<'_, Cerebro>,
    f: impl FnOnce(&mut Brain) -> obrain_api::Result<T>,
) -> Result<T, String> {
    let mut guarda = cerebro.0.lock().map_err(texto)?;
    let brain = guarda.as_mut().ok_or_else(|| CERRADA.to_string())?;
    f(brain).map_err(texto)
}

/// Busca; con `coleccion`, sólo dentro de ella (ADR-0031).
#[tauri::command]
fn vistazo(
    query: String,
    limit: usize,
    coleccion: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<Glance, String> {
    let coleccion = coleccion.map(EntityId);
    con(&cerebro, |b| b.glance_in(&query, limit, coleccion.as_ref()))
}

// ─────────────────────────────────────────────── tus colecciones (ADR-0031)
//
// Todas reciben identificadores —de la colección y de los documentos—, nunca
// rutas, como abrir y revelar. Y ninguna toca un archivo.

#[tauri::command]
fn colecciones(cerebro: State<'_, Cerebro>) -> Result<Vec<Coleccion>, String> {
    con(&cerebro, |b| b.collections())
}

#[tauri::command]
fn coleccion(id: String, cerebro: State<'_, Cerebro>) -> Result<Abierta, String> {
    con(&cerebro, |b| b.collection(&EntityId(id)))
}

#[tauri::command]
fn crear_coleccion(nombre: String, cerebro: State<'_, Cerebro>) -> Result<String, String> {
    con(&cerebro, |b| b.create_collection(&nombre, "user@ui"))
}

#[tauri::command]
fn renombrar_coleccion(
    id: String,
    nombre: String,
    cerebro: State<'_, Cerebro>,
) -> Result<(), String> {
    con(&cerebro, |b| {
        b.rename_collection(&EntityId(id), &nombre, "user@ui")
    })
}

#[tauri::command]
fn borrar_coleccion(id: String, cerebro: State<'_, Cerebro>) -> Result<usize, String> {
    con(&cerebro, |b| b.delete_collection(&EntityId(id)))
}

#[tauri::command]
fn meter_en_coleccion(
    id: String,
    docs: Vec<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<usize, String> {
    let docs: Vec<EntityId> = docs.into_iter().map(EntityId).collect();
    con(&cerebro, |b| {
        b.add_to_collection(&EntityId(id), &docs, "user@ui")
    })
}

#[tauri::command]
fn sacar_de_coleccion(
    id: String,
    docs: Vec<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<usize, String> {
    let docs: Vec<EntityId> = docs.into_iter().map(EntityId).collect();
    con(&cerebro, |b| b.remove_from_collection(&EntityId(id), &docs))
}

/// Arrastrar de una colección a otra (ADR-0033): entran en `hacia` y salen de
/// `desde`, de una vez.
#[tauri::command]
fn mover_entre_colecciones(
    desde: String,
    hacia: String,
    docs: Vec<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<obrain_api::Movidos, String> {
    let docs: Vec<EntityId> = docs.into_iter().map(EntityId).collect();
    con(&cerebro, |b| {
        b.move_between_collections(&EntityId(desde), &EntityId(hacia), &docs, "user@ui")
    })
}

#[tauri::command]
fn lente(
    focus: Vec<String>,
    radius: u8,
    budget: usize,
    cerebro: State<'_, Cerebro>,
) -> Result<Lens, String> {
    let focus: Vec<EntityId> = focus.into_iter().map(EntityId).collect();
    con(&cerebro, |b| b.lens(&focus, radius, budget))
}

/// El cielo: lo que se ve al abrir, antes de que nadie escriba nada.
#[tauri::command]
fn cielo(target: usize, cerebro: State<'_, Cerebro>) -> Result<Sky, String> {
    con(&cerebro, |b| b.sky(target))
}

/// El explorador de ADR-0014: qué hay dentro de esto.
///
/// `foco` a `None` entra por las raíces, que es donde empieza quien abre el
/// explorador sin haber enfocado nada en la galaxia.
#[tauri::command]
fn explorar(foco: Option<String>, cerebro: State<'_, Cerebro>) -> Result<Listing, String> {
    let foco = foco.map(EntityId);
    con(&cerebro, |b| b.children(foco.as_ref()))
}

/// Lo último que tocaste: el «Recientes» del inicio.
#[tauri::command]
fn recientes(limit: usize, cerebro: State<'_, Cerebro>) -> Result<Vec<Row>, String> {
    con(&cerebro, |b| b.recent(limit))
}

/// Lo que la puerta retiene: la vista «Sensibles».
#[tauri::command]
fn sensibles(cerebro: State<'_, Cerebro>) -> Result<Vec<Row>, String> {
    con(&cerebro, |b| b.sensitive())
}

/// Lo que caduca pronto o ya caducó: la tarjeta del inicio (ADR-0023). Con
/// el PIN bloqueado, lo sensible llega sin fecha: lo decide el núcleo.
#[tauri::command]
fn vencimientos(cerebro: State<'_, Cerebro>) -> Result<Vec<Expiring>, String> {
    con(&cerebro, |b| b.expiring())
}

/// «cambiar» en la vista previa: la fecha en que caduca un documento
/// (`AAAA-MM-DD`), o `None` para «no caduca». Una aserción del usuario, que
/// gana para siempre (I5), como [`corregir`]; va aparte porque el núcleo
/// comprueba que la fecha exista y que lo sensible esté desbloqueado.
#[tauri::command]
fn corregir_vencimiento(
    id: String,
    fecha: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<String, String> {
    con(&cerebro, |b| b.set_expiry(&EntityId(id), fecha.as_deref()))
}

/// Lo que ya no está en tu bóveda: la vista «Papelera» (ADR-0025).
#[tauri::command]
fn papelera(cerebro: State<'_, Cerebro>) -> Result<Vec<EnPapelera>, String> {
    con(&cerebro, |b| b.trash())
}

/// «Olvidar ya» (con `id`) o «Vaciar papelera» (sin él): borra el texto y los
/// vectores que obrain guardaba de lo que ya no está. **No toca tus archivos**
/// —ya no están— ni el log (I4). Devuelve cuántos se olvidaron.
#[tauri::command]
fn olvidar(id: Option<String>, cerebro: State<'_, Cerebro>) -> Result<usize, String> {
    con(&cerebro, |b| b.forget(id.map(EntityId).as_ref()))
}

/// Las carpetas que marcaste como sensibles, para la vista «Sensibles».
#[tauri::command]
fn carpetas_marcadas(cerebro: State<'_, Cerebro>) -> Result<Vec<MarkedFolder>, String> {
    con(&cerebro, |b| b.marked_folders())
}

/// El botón de la cabecera de una carpeta: marcarla como sensible —todo lo
/// que tenga dentro lo hereda, también lo que llegue después— o, con `nivel`
/// a `None`, quitarle la marca. Una aserción del usuario (I5), como
/// [`corregir`]; la diferencia es que ésta recalcula la carpeta entera en una
/// transacción, y eso lo decide el núcleo, no este comando (ADR-0020).
#[tauri::command]
fn marcar_carpeta(
    carpeta: String,
    nivel: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<FolderMarked, String> {
    let nivel = match nivel.as_deref() {
        None => None,
        Some(n) => Some(Sensitivity::parse(n).ok_or_else(|| format!("nivel desconocido: {n}"))?),
    };
    con(&cerebro, |b| b.mark_folder(&EntityId(carpeta), nivel))
}

#[tauri::command]
fn entidad(id: String, cerebro: State<'_, Cerebro>) -> Result<EntityView, String> {
    con(&cerebro, |b| b.entity(&EntityId(id)))
}

/// Qué se ve del archivo. La segunda mitad de M5b.
///
/// `consulta` sólo decide **dónde se abre** el texto —el tramo que explica lo
/// que se buscó— y nunca cuánto se puede leer. Lo que se puede leer lo decide
/// la guarda de M1 dentro del núcleo, y no la toca ningún argumento de aquí.
/// Con PIN, lo sensible llega vacío hasta desbloquear (ADR-0021). `tramo`
/// abre por un sitio exacto: dónde está un documento dentro de su paquete
/// (ADR-0030).
#[tauri::command]
fn previsualizar(
    id: String,
    consulta: Option<String>,
    tramo: Option<[usize; 2]>,
    expected_hash: Option<String>,
    expected_text_hash: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<Preview, String> {
    con(&cerebro, |b| {
        b.preview_at_version(
            &EntityId(id),
            consulta.as_deref(),
            tramo.map(|[x, y]| (x, y)),
            expected_hash.as_deref(),
            expected_text_hash.as_deref(),
        )
    })
}

/// La ruta de una entidad, según el log. **La pone el núcleo, no la ventana**,
/// y sólo si se puede abrir: con PIN, lo sensible pide desbloquear antes.
fn operar_archivo(
    cerebro: &State<'_, Cerebro>,
    id: String,
    expected_hash: Option<String>,
    accion: impl FnOnce(String) -> Result<(), String>,
) -> Result<(), String> {
    let guarda = cerebro.0.lock().map_err(texto)?;
    let brain = guarda.as_ref().ok_or_else(|| CERRADA.to_string())?;
    let ruta = brain
        .path_to_open_version(&EntityId(id), expected_hash.as_deref())
        .map_err(texto)?
        .ok_or_else(|| "esta entidad no es un archivo con ruta".to_string())?;
    accion(ruta)
}

/// Abre el archivo con el programa que el sistema tenga asociado.
///
/// Tiene dos costes y los dos se dicen en pantalla, no sólo aquí:
///
/// 1. **obrain es solo-lectura; Word no tiene por qué serlo.** Es solo-lectura
///    *del sistema*, no *del acto*.
/// 2. **Si el archivo está en la nube, abrirlo lo descarga.** No se impide, y
///    es la única excepción que la regla de M5b permite: hidratar jamás como
///    efecto secundario, **sólo como acto explícito del usuario y dicho con
///    todas las letras**. Decirlo es responsabilidad del botón que llama aquí.
#[tauri::command]
fn abrir(
    id: String,
    expected_hash: Option<String>,
    app: tauri::AppHandle,
    cerebro: State<'_, Cerebro>,
) -> Result<(), String> {
    operar_archivo(&cerebro, id, expected_hash, |ruta| {
        app.opener().open_path(ruta, None::<&str>).map_err(texto)
    })
}

/// Lo revela en el explorador de Windows: abre su carpeta y lo selecciona.
///
/// Nunca ejecuta el archivo, así que no hereda el primer coste de [`abrir`].
#[tauri::command]
fn revelar(
    id: String,
    expected_hash: Option<String>,
    app: tauri::AppHandle,
    cerebro: State<'_, Cerebro>,
) -> Result<(), String> {
    operar_archivo(&cerebro, id, expected_hash, |ruta| {
        app.opener().reveal_item_in_dir(ruta).map_err(texto)
    })
}

/// La corrección de I5. Escribe una aserción del usuario, que gana para
/// siempre. Es el único comando que **escribe** en el log, además de marcar
/// una carpeta y de [`corregir_vencimiento`].
#[tauri::command]
fn corregir(
    subject: String,
    predicate: String,
    object: ObjetoCorreccion,
    cerebro: State<'_, Cerebro>,
) -> Result<String, String> {
    con(&cerebro, |b| {
        b.correct(
            &EntityId(subject),
            &predicate,
            match object {
                ObjetoCorreccion::Text(v) => obrain_core::Value::Text(v),
                ObjetoCorreccion::Entity(v) => obrain_core::Value::Entity(EntityId(v)),
            },
        )
    })
}

#[derive(serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
enum ObjetoCorreccion {
    Text(String),
    Entity(String),
}

/// El cliente identifica la inferencia; nunca reenvía su etiqueta como objeto.
#[tauri::command]
fn confirmar(
    subject: String,
    assertion: String,
    cerebro: State<'_, Cerebro>,
) -> Result<String, String> {
    con(&cerebro, |b| b.confirm(&EntityId(subject), &assertion))
}

#[tauri::command]
fn estado(
    cerebro: State<'_, Cerebro>,
    indexando: State<'_, Indexando>,
    ocr: State<'_, MotorOcr>,
) -> Result<Estado, String> {
    let guarda = cerebro.0.lock().map_err(texto)?;
    let db = default_db_path().display().to_string();
    let ocr = ocr.0.clone();
    let Some(brain) = guarda.as_ref() else {
        return Ok(Estado {
            db,
            cerrada: true,
            semantic: false,
            entities: 0,
            assertions: 0,
            libraries: Vec::new(),
            indexing: false,
            ocr,
        });
    };
    let store = brain.store();
    let entities: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))
        .map_err(texto)?;
    Ok(Estado {
        db,
        cerrada: false,
        semantic: brain.semantic_ready(),
        entities,
        assertions: store.log_len().map_err(texto)?,
        libraries: obrain_api::indexar::libraries(store).map_err(texto)?,
        indexing: indexando.0.load(Ordering::SeqCst),
        ocr,
    })
}

// ───────────────────────────────────────────────────── seguridad (ADR-0021)

/// Cómo está la seguridad: cifrado, clave de recuperación pendiente, PIN.
#[tauri::command]
fn seguridad(cerebro: State<'_, Cerebro>) -> Result<EstadoSeguridad, String> {
    con(&cerebro, |b| b.security())
}

/// La clave de recuperación, para enseñarla. Con PIN, desbloqueado.
#[tauri::command]
fn clave_recuperacion(cerebro: State<'_, Cerebro>) -> Result<Option<String>, String> {
    con(&cerebro, |b| b.recovery_code())
}

/// «Ya la guardé»: la app deja de enseñarla al abrir.
#[tauri::command]
fn recuperacion_guardada(cerebro: State<'_, Cerebro>) -> Result<(), String> {
    con(&cerebro, |b| b.confirm_recovery_saved())
}

/// Abre la base con la clave de recuperación cuando Windows no tiene su
/// llave, y la deja otra vez en manos de Windows. Después, todo arranca como
/// si se hubiera abierto normal.
#[tauri::command]
fn recuperar(codigo: String, app: AppHandle, cerebro: State<'_, Cerebro>) -> Result<(), String> {
    let db = default_db_path();
    {
        let mut guarda = cerebro.0.lock().map_err(texto)?;
        if guarda.is_some() {
            return Ok(());
        }
        seguridad::recuperar(&db, &codigo).map_err(texto)?;
        *guarda = Some(Brain::open(&db).map_err(texto)?);
    }
    app.state::<Indexando>().0.store(true, Ordering::SeqCst);
    arrancar_en_fondo(app);
    Ok(())
}

#[tauri::command]
fn pin_desbloquear(pin: String, cerebro: State<'_, Cerebro>) -> Result<(), String> {
    con(&cerebro, |b| b.unlock(&pin))
}

#[tauri::command]
fn pin_bloquear(cerebro: State<'_, Cerebro>) -> Result<(), String> {
    con(&cerebro, |b| {
        b.lock();
        Ok(())
    })
}

#[tauri::command]
fn pin_poner(
    nuevo: String,
    actual: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<(), String> {
    con(&cerebro, |b| b.set_pin(&nuevo, actual.as_deref()))
}

#[tauri::command]
fn pin_quitar(actual: String, cerebro: State<'_, Cerebro>) -> Result<(), String> {
    con(&cerebro, |b| b.remove_pin(&actual))
}

/// Olvidé el PIN: con la clave de recuperación se pone otro (o ninguno).
#[tauri::command]
fn pin_restablecer(
    codigo: String,
    nuevo: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<(), String> {
    con(&cerebro, |b| b.reset_pin(&codigo, nuevo.as_deref()))
}

// ───────────────────────────────────────────── respaldos y restauración

/// Libera automáticamente un cerrojo atómico (`false`) al salir del ámbito
/// únicamente cuando fue adquirido por esta operación.
struct GuardiaAtomica<'a>(&'a AtomicBool);

impl Drop for GuardiaAtomica<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Crea un respaldo consistente, cifrado y verificado de la base activa.
///
/// El diálogo de carpeta lo abre Rust en el sistema operativo; el webview
/// no recibe ni envía rutas directas del disco. Genera una subcarpeta con
/// marca temporal para no sobrescribir ningún respaldo previo.
#[tauri::command]
async fn respaldo_crear(
    app: AppHandle,
    cerebro: State<'_, Cerebro>,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> Result<Option<ResultadoCrearRespaldo>, String> {
    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }
    if cerebro.0.lock().map(|g| g.is_none()).unwrap_or(true) {
        return Err(CERRADA.to_string());
    }

    let a = app.clone();
    let elegida = tauri::async_runtime::spawn_blocking(move || {
        a.dialog()
            .file()
            .set_title("Elige la carpeta donde guardar el respaldo")
            .blocking_pick_folder()
    })
    .await
    .map_err(texto)?;

    let Some(carpeta) = elegida else {
        return Ok(None);
    };
    let carpeta_padre = carpeta.into_path().map_err(texto)?;

    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let nombre_subcarpeta = format!("obrain-respaldo-{}", timestamp);
    let destino = carpeta_padre.join(&nombre_subcarpeta);

    let db = default_db_path();
    let d = destino.clone();
    let manifiesto = tauri::async_runtime::spawn_blocking(move || {
        respaldo::crear(&db, &d)
    })
    .await
    .map_err(texto)?
    .map_err(texto)?;

    let nombre = destino
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| destino.display().to_string());
    let ruta_str = destino.display().to_string();

    if let Ok(mut sel) = respaldo_state.seleccionada.lock() {
        *sel = Some(destino);
    }

    Ok(Some(ResultadoCrearRespaldo {
        carpeta: ruta_str,
        nombre_carpeta: nombre,
        manifiesto,
    }))
}

/// Selecciona una carpeta de respaldo existente mediante el diálogo nativo de Windows.
///
/// Verifica inmediatamente el manifiesto y hashes BLAKE3 de sus archivos,
/// y determina si la base requiere clave de recuperación para abrirse en esta máquina.
#[tauri::command]
async fn respaldo_seleccionar(
    clave: Option<String>,
    app: AppHandle,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> Result<Option<RespaldoInfoResumen>, String> {
    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }

    let a = app.clone();
    let elegida = tauri::async_runtime::spawn_blocking(move || {
        a.dialog()
            .file()
            .set_title("Selecciona la carpeta de tu respaldo")
            .blocking_pick_folder()
    })
    .await
    .map_err(texto)?;

    let Some(carpeta) = elegida else {
        return Ok(None);
    };
    let ruta = carpeta.into_path().map_err(texto)?;

    if let Ok(mut sel) = respaldo_state.seleccionada.lock() {
        *sel = None;
    }

    let r = ruta.clone();
    let c = clave.clone();
    let (manifiesto, requiere_clave) = tauri::async_runtime::spawn_blocking(move || {
        match respaldo::verificar(&r) {
            Ok(m) => Ok::<_, respaldo::Error>((m, false)),
            Err(respaldo::Error::Keys(_)) if c.is_some() => {
                let m = respaldo::verificar_con_clave(&r, c.as_deref())?;
                Ok((m, true))
            }
            Err(e) => Err(e),
        }
    })
    .await
    .map_err(texto)?
    .map_err(texto)?;

    let nombre = ruta
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| ruta.display().to_string());
    let ruta_str = ruta.display().to_string();

    if let Ok(mut sel) = respaldo_state.seleccionada.lock() {
        *sel = Some(ruta);
    }

    Ok(Some(RespaldoInfoResumen {
        carpeta: ruta_str,
        nombre_carpeta: nombre,
        manifiesto,
        requiere_clave,
    }))
}

/// Comprueba la integridad exhaustiva del respaldo seleccionado (hashes, páginas HMAC, B-trees).
#[tauri::command]
async fn respaldo_comprobar(
    clave: Option<String>,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> Result<Manifiesto, String> {
    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }
    let ruta = {
        let guarda = respaldo_state.seleccionada.lock().map_err(texto)?;
        guarda.as_ref().cloned().ok_or_else(|| "No hay ninguna carpeta de respaldo seleccionada".to_string())?
    };
    tauri::async_runtime::spawn_blocking(move || {
        respaldo::verificar_con_clave(&ruta, clave.as_deref()).map_err(texto)
    })
    .await
    .map_err(texto)?
}

/// Ensaya de forma aislada la apertura y recuperación del respaldo en un entorno temporal,
/// sin tocar en ningún momento el perfil activo.
#[tauri::command]
async fn respaldo_ensayar(
    clave: Option<String>,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> Result<InformeEnsayo, String> {
    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }
    let ruta = {
        let guarda = respaldo_state.seleccionada.lock().map_err(texto)?;
        guarda.as_ref().cloned().ok_or_else(|| "No hay ninguna carpeta de respaldo seleccionada".to_string())?
    };
    tauri::async_runtime::spawn_blocking(move || {
        respaldo::ensayar(&ruta, None, clave.as_deref(), None).map_err(texto)
    })
    .await
    .map_err(texto)?
}

/// Diagnostica y elabora el plan de sustitución antes de una restauración real.
///
/// Compara el respaldo contra el perfil activo, advierte si la base activa
/// tiene novedades o más aserciones, y reporta el estado de presencia local
/// de las bibliotecas registradas en el respaldo.
#[tauri::command]
async fn restauracion_planificar(
    clave: Option<String>,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> Result<PlanRestauracion, String> {
    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }
    let ruta = {
        let guarda = respaldo_state.seleccionada.lock().map_err(texto)?;
        guarda.as_ref().cloned().ok_or_else(|| "No hay ninguna carpeta de respaldo seleccionada".to_string())?
    };
    let db = default_db_path();
    tauri::async_runtime::spawn_blocking(move || {
        respaldo::planificar(&ruta, &db, clave.as_deref()).map_err(texto)
    })
    .await
    .map_err(texto)?
}

/// Ejecuta la restauración coordinada de la base sobre el perfil activo.
///
/// Requiere autorización explícita (`confirmar = true`).
/// Coordina la detención del vigía, espera al indexador si estuviera trabajando,
/// libera la conexión de Brain, crea salvaguarda completa y ejecuta el protocolo
/// con reversibilidad automática y recibo de transición en disco.
#[tauri::command]
async fn restauracion_ejecutar(
    clave: Option<String>,
    confirmar: bool,
    app: AppHandle,
    cerebro: State<'_, Cerebro>,
    indexando: State<'_, Indexando>,
    vigilante: State<'_, Vigilante>,
    pendiente: State<'_, Pendiente>,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> Result<InformeRestauracion, String> {
    if !confirmar {
        return Err("La restauración requiere autorización explícita de la persona.".to_string());
    }

    if respaldo_state.restaurando.swap(true, Ordering::SeqCst) {
        return Err("Ya hay una operación de restauración en marcha.".to_string());
    }
    let _guardia_restaurando = GuardiaAtomica(&respaldo_state.restaurando);

    let carpeta = {
        let guarda = respaldo_state.seleccionada.lock().map_err(texto)?;
        guarda.as_ref().cloned().ok_or_else(|| "No hay ninguna carpeta de respaldo seleccionada".to_string())?
    };

    let res = coordinar_restauracion(
        carpeta,
        default_db_path(),
        clave,
        &cerebro,
        &indexando,
        &vigilante,
        &pendiente,
        std::time::Duration::from_secs(30),
    )
    .await;

    // Si Brain sigue abierto tras la operación (éxito o recuperación intacta tras
    // un fallo previo a la sustitución) y el vigía fue detenido, reactivarlo.
    let brain_abierto = cerebro.0.lock().map(|g| g.is_some()).unwrap_or(false);
    let vigia_detenido = vigilante.0.lock().map(|g| g.is_none()).unwrap_or(false);
    if brain_abierto && vigia_detenido {
        reiniciar_vigia(&app, &vigilante);
    }

    if let Ok(ref informe) = res {
        let _ = app.emit("restaurado", informe);
    }

    res
}

async fn coordinar_restauracion(
    carpeta: PathBuf,
    db: PathBuf,
    clave: Option<String>,
    cerebro: &Cerebro,
    indexando: &Indexando,
    vigilante: &Vigilante,
    pendiente: &Pendiente,
    plazo_indexador: std::time::Duration,
) -> Result<InformeRestauracion, String> {
    // 1. Adquirir atómicamente el cerrojo `Indexando`. Si un trabajador está activo,
    //    esperar hasta `plazo_indexador`. Si expira el plazo, abortar sin liberar
    //    el cerrojo que pertenece al indexador ni tocar Brain o el vigía.
    let inicio = std::time::Instant::now();
    while indexando.0.swap(true, Ordering::SeqCst) {
        if inicio.elapsed() >= plazo_indexador {
            return Err("El indexador de bibliotecas está ocupado. Espera a que termine e inténtalo de nuevo.".to_string());
        }
        tauri::async_runtime::spawn_blocking(|| {
            std::thread::sleep(std::time::Duration::from_millis(25));
        })
        .await
        .map_err(texto)?;
    }
    let _guardia_indexando = GuardiaAtomica(&indexando.0);
    pendiente.0.store(false, Ordering::SeqCst);

    // 2. Detener el vigilante de bibliotecas soltándolo
    if let Ok(mut vigia_guard) = vigilante.0.lock() {
        let _ = vigia_guard.take();
    }

    // 3. Extraer el modelo semántico, cerrar Brain sincrónicamente para liberar
    //    su descriptor SQLite y recordar si había un perfil abierto previamente.
    let existia_db_antes = db.exists();
    let (tenia_brain_abierto, mut modelo_previo) = {
        let mut guarda = cerebro.0.lock().map_err(texto)?;
        let mut brain_previo = guarda.take();
        let modelo = brain_previo.as_mut().and_then(Brain::take_embedder);
        let abierto = brain_previo.is_some();
        drop(brain_previo);
        (abierto, modelo)
    };

    // 4. Restauración con verificación de exclusividad, salvaguarda y recibo en disco
    let db_clon = db.clone();
    let carpeta_clon = carpeta.clone();
    let opciones = OpcionesRestauracion {
        codigo_recuperacion: clave,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };

    let res = tauri::async_runtime::spawn_blocking(move || {
        respaldo::restaurar(&carpeta_clon, &db_clon, opciones)
    })
    .await
    .map_err(texto)?;

    let mut reabrir_cerebro = |ruta_db: &PathBuf| -> Result<(), obrain_api::Error> {
        let mut b = Brain::open(ruta_db)?;
        if let Some(m) = modelo_previo.take() {
            b.set_embedder(m);
        }
        if let Ok(mut g) = cerebro.0.lock() {
            *g = Some(b);
        }
        Ok(())
    };

    match res {
        Ok(informe) => {
            match reabrir_cerebro(&db) {
                Ok(()) => Ok(informe),
                Err(e) => {
                    eprintln!("error abriendo base restaurada: {e}");
                    Err(format!("El perfil se restauró, pero ocurrió un error al abrirlo: {e}"))
                }
            }
        }
        Err(respaldo::Error::RestauracionFallidaYRecuperada(motivo)) => {
            if tenia_brain_abierto && existia_db_antes && db.exists() {
                let _ = reabrir_cerebro(&db);
            }
            Err(format!("No se pudo completar la restauración ({motivo}); se ha recuperado tu perfil anterior intacto."))
        }
        Err(respaldo::Error::TransicionAmbigua(p)) => {
            eprintln!("transición ambigua detectada en {}", p.display());
            Err(format!("Transición interrumpida con estado ambiguo en {}. Se detuvo la apertura automática y se preservan las evidencias intactas.", p.display()))
        }
        Err(respaldo::Error::RestauracionCritica(e1, e2, p)) => {
            eprintln!("restauración crítica: {e1}, {e2}, {}", p.display());
            Err(format!("Fallo crítico ({e1}). Se detuvo la apertura automática y los archivos originales se conservan en {}.", p.display()))
        }
        Err(e) => {
            if tenia_brain_abierto && existia_db_antes && db.exists() {
                let _ = reabrir_cerebro(&db);
            }
            Err(texto(e))
        }
    }
}

fn reiniciar_vigia(app: &AppHandle, vigilante: &State<'_, Vigilante>) {
    let handle = app.clone();
    let vigia = Vigia::empezar(vigilar::PAUSA, move || por_cambio(&handle))
        .map_err(|e| eprintln!("no se puede vigilar tus carpetas tras restauración: {e}"))
        .ok();
    if let Ok(mut g) = vigilante.0.lock() {
        *g = vigia;
    }
    if let Ok(g) = app.state::<Cerebro>().0.lock() {
        if let Some(b) = g.as_ref() {
            if let (Ok(bibliotecas), Ok(v)) = (
                obrain_api::indexar::libraries(b.store()),
                app.state::<Vigilante>().0.lock(),
            ) {
                if let Some(vigia) = v.as_ref() {
                    let b_paths: Vec<PathBuf> = bibliotecas.into_iter().map(PathBuf::from).collect();
                    vigia.sincronizar(&b_paths);
                }
            }
        }
    }
}

/// Reanuda la indexación automática tras una restauración, una vez que el usuario
/// ha confirmado sus bibliotecas.
#[tauri::command]
fn reanudar_reindexado(cerebro: State<'_, Cerebro>) -> Result<(), String> {
    let mut guarda = cerebro.0.lock().map_err(texto)?;
    let brain = guarda.as_mut().ok_or_else(|| CERRADA.to_string())?;
    seguridad::reanudar_reindexado(brain.store().conn()).map_err(texto)
}

/// Consulta si el reindexado está pausado y el motivo registrado en la base.
#[tauri::command]
fn motivo_reindexado_pausado(cerebro: State<'_, Cerebro>) -> Result<Option<String>, String> {
    let mut guarda = cerebro.0.lock().map_err(texto)?;
    let brain = guarda.as_mut().ok_or_else(|| CERRADA.to_string())?;
    seguridad::motivo_reindexado_pausado(brain.store().conn()).map_err(texto)
}

/// Revela la carpeta de salvaguarda en el explorador de archivos.
#[tauri::command]
fn abrir_carpeta_salvaguarda(ruta: String, app: AppHandle) -> Result<(), String> {
    let p = PathBuf::from(&ruta);
    if p.exists() {
        app.opener().reveal_item_in_dir(ruta).map_err(texto)
    } else {
        Err("La carpeta de salvaguarda no existe o ha sido movida".to_string())
    }
}

// ───────────────────────────────────────────────────────────── indexar

/// Vuelve a leer tus bibliotecas. **Vuelve al instante**: el trabajo va en un
/// hilo y el resultado llega con el evento `indexado`. Devuelve `false` si ya
/// había uno en marcha (o si la base está cerrada).
#[tauri::command]
fn actualizar(
    app: AppHandle,
    cerebro: State<'_, Cerebro>,
    indexando: State<'_, Indexando>,
    respaldo_state: State<'_, EstadoRespaldo>,
) -> bool {
    if respaldo_state.restaurando.load(Ordering::SeqCst) {
        return false;
    }
    if cerebro.0.lock().map(|g| g.is_none()).unwrap_or(true) {
        return false;
    }
    if indexando.0.swap(true, Ordering::SeqCst) {
        return false;
    }
    if respaldo_state.restaurando.load(Ordering::SeqCst)
        || cerebro.0.lock().map(|g| g.is_none()).unwrap_or(true)
    {
        indexando.0.store(false, Ordering::SeqCst);
        return false;
    }
    // El modelo se presta: vectorizar con el cerebro cogido dejaría la barra
    // sin responder. Mientras tanto la búsqueda sigue, en léxico.
    let modelo = cerebro
        .0
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().and_then(Brain::take_embedder));
    let _ = app.emit("indexando", ());
    std::thread::spawn(move || indexar_en_fondo(&app, None, modelo, false));
    true
}

/// Añade una carpeta a tu bóveda y la indexa.
///
/// **El selector de carpetas lo abre Rust.** La ventana sólo pide «añade una
/// carpeta»; ninguna ruta viaja desde el webview hasta aquí, así que la
/// ventana no puede mandar indexar un sitio del disco que la persona no haya
/// elegido con sus manos. Devuelve `false` si se canceló o si ya había un
/// reindexado en marcha; el resultado llega con el evento `indexado`.
#[tauri::command]
async fn anadir_biblioteca(app: AppHandle) -> Result<bool, String> {
    if app.state::<EstadoRespaldo>().restaurando.load(Ordering::SeqCst) {
        return Ok(false);
    }
    if app.state::<Indexando>().0.load(Ordering::SeqCst) {
        return Ok(false);
    }
    {
        let guarda = app.state::<Cerebro>().inner().0.lock().map_err(texto)?;
        let Some(brain) = guarda.as_ref() else {
            return Err(CERRADA.to_string());
        };
        if seguridad::reindexado_pausado(brain.store().conn()).unwrap_or(false) {
            return Err(
                "la sincronización está en pausa tras la restauración; reanúdala antes de añadir carpetas"
                    .to_string(),
            );
        }
    }
    // El diálogo bloquea hasta que se elige: fuera del hilo de la ventana.
    let a = app.clone();
    let elegida = tauri::async_runtime::spawn_blocking(move || {
        a.dialog()
            .file()
            .set_title("Elige la carpeta de tu bóveda")
            .blocking_pick_folder()
    })
    .await
    .map_err(texto)?;
    let Some(carpeta) = elegida else {
        return Ok(false);
    };
    let ruta = carpeta.into_path().map_err(texto)?;
    if app.state::<EstadoRespaldo>().restaurando.load(Ordering::SeqCst) {
        return Ok(false);
    }
    if app.state::<Indexando>().0.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    if app.state::<EstadoRespaldo>().restaurando.load(Ordering::SeqCst)
        || app
            .state::<Cerebro>()
            .0
            .lock()
            .map(|g| g.is_none())
            .unwrap_or(true)
    {
        app.state::<Indexando>().0.store(false, Ordering::SeqCst);
        return Ok(false);
    }
    let modelo = app
        .state::<Cerebro>()
        .0
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().and_then(Brain::take_embedder));
    let _ = app.emit("indexando", ());
    std::thread::spawn(move || indexar_en_fondo(&app, Some(vec![ruta]), modelo, false));
    Ok(true)
}

/// El indexado de verdad. Siempre en un hilo aparte, con su propia conexión.
///
/// `raices` a `None` vuelve a leer todas las bibliotecas; con carpetas, las
/// añade (y deja las demás como estaban). La conexión propia abre la base
/// cifrada igual que la de la ventana: la llave la da Windows.
fn indexar_en_fondo(
    app: &AppHandle,
    raices: Option<Vec<PathBuf>>,
    modelo: Option<Box<dyn Embedder + Send>>,
    vivo: bool,
) {
    let res = Store::open(default_db_path())
        .map_err(texto)
        .and_then(|mut store| {
            Brain::initialize_store(&mut store).map_err(texto)?;
            let m = modelo.as_deref().map(|m| m as &dyn Embedder);
            let r = match &raices {
                Some(r) => obrain_api::indexar::index(&mut store, r, m),
                None => obrain_api::indexar::reindex(&mut store, m),
            }
            .map_err(texto)?;
            // El vigía mira exactamente tus bibliotecas: también la que se
            // acaba de añadir.
            let vigilante = app.state::<Vigilante>();
            if let (Ok(bibliotecas), Ok(vigia)) =
                (obrain_api::indexar::libraries(&store), vigilante.0.lock())
            {
                if let Some(v) = vigia.as_ref() {
                    let b: Vec<PathBuf> = bibliotecas.into_iter().map(PathBuf::from).collect();
                    v.sincronizar(&b);
                }
            }
            Ok(r)
        });
    // El modelo vuelve al cerebro pase lo que pase.
    if let Some(m) = modelo {
        if let Ok(mut g) = app.state::<Cerebro>().0.lock() {
            if let Some(b) = g.as_mut() {
                b.set_embedder(m);
            }
        }
    }
    app.state::<Indexando>().0.store(false, Ordering::SeqCst);
    let aviso = match res {
        Ok(r) => Indexado {
            report: Some(r),
            error: None,
            live: vivo,
        },
        Err(e) => {
            eprintln!("no se pudo actualizar: {e}");
            Indexado {
                report: None,
                error: Some(e),
                live: vivo,
            }
        }
    };
    let _ = app.emit("indexado", aviso);
    // Lo que cambió mientras tanto, en una pasada más (salvo que haya comenzado una restauración).
    if app.state::<Pendiente>().0.swap(false, Ordering::SeqCst)
        && !app.state::<EstadoRespaldo>().restaurando.load(Ordering::SeqCst)
    {
        por_cambio(app);
    }
}

/// El modelo, fuera del cerrojo, y después **tus bibliotecas al día**: lo que
/// añadiste a la carpeta mientras la aplicación estaba cerrada aparece sin
/// pulsar nada. Si el modelo no está descargado, se indexa igual y la barra
/// sigue respondiendo en léxico.
fn arrancar_en_fondo(handle: AppHandle) {
    std::thread::spawn(move || {
        let modelo = match obrain_api::load_model(None) {
            Ok(Some(m)) => Some(m),
            Ok(None) => {
                eprintln!(
                    "sin modelo local: la busqueda va en lexico. \
                     `obrain-embed <db> --descargar-modelo` lo enciende."
                );
                None
            }
            Err(e) => {
                eprintln!("no se pudo cargar el modelo: {e}");
                None
            }
        };
        indexar_en_fondo(&handle, None, modelo, false);
    });
}

/// **Algo cambió en tus bibliotecas** y lleva la pausa quieto: la misma
/// pasada que «actualizar». Si ya hay una en marcha, se anota y sale al
/// terminar ésa: un archivo guardado después de que el escáner pasara por su
/// carpeta no puede quedarse fuera.
fn cerrada_o_en_pausa(cerebro: &Cerebro) -> bool {
    cerebro
        .0
        .lock()
        .map(|g| {
            g.as_ref().is_none_or(|b| {
                seguridad::reindexado_pausado(b.store().conn()).unwrap_or(false)
            })
        })
        .unwrap_or(true)
}

fn por_cambio(app: &AppHandle) {
    if app.state::<EstadoRespaldo>().restaurando.load(Ordering::SeqCst) {
        return;
    }
    if cerrada_o_en_pausa(app.state::<Cerebro>().inner()) {
        return;
    }
    if app.state::<Indexando>().0.swap(true, Ordering::SeqCst) {
        app.state::<Pendiente>().0.store(true, Ordering::SeqCst);
        return;
    }
    if app.state::<EstadoRespaldo>().restaurando.load(Ordering::SeqCst)
        || cerrada_o_en_pausa(app.state::<Cerebro>().inner())
    {
        app.state::<Indexando>().0.store(false, Ordering::SeqCst);
        return;
    }
    let _ = app.emit("indexando", ());
    let modelo = app
        .state::<Cerebro>()
        .0
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().and_then(Brain::take_embedder));
    let app = app.clone();
    std::thread::spawn(move || indexar_en_fondo(&app, None, modelo, true));
}

fn main() {
    tauri::Builder::default()
        // Sin permisos para el webview a propósito: las dos salidas se invocan
        // desde `abrir` y `revelar`, que son Rust y consultan el log.
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let db = default_db_path();
            if let Some(dir) = db.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            // Abre (y cifra, si venía en claro). Sin la llave de Windows no es
            // un fallo de arranque: la ventana pedirá la clave de recuperación.
            let brain = match Brain::open(&db) {
                Ok(b) => Some(b),
                Err(e) if e.is_key_unavailable() => {
                    eprintln!("{e}");
                    None
                }
                Err(e) => {
                    // setup corre en el hilo principal: el diálogo debe ser
                    // asíncrono y el proceso seguir vivo hasta que se lea.
                    let handle = app.handle().clone();
                    app.dialog()
                        .message(e.to_string())
                        .title("No se ha podido abrir la bóveda")
                        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                        .show(move |_| handle.exit(1));
                    None
                }
            };
            let abierta = brain.is_some();
            app.manage(Cerebro(Mutex::new(brain)));
            // Arranca marcado: el hilo de abajo reindexa en cuanto tenga modelo.
            app.manage(Indexando(AtomicBool::new(abierta)));
            app.manage(MotorOcr(obrain_api::ocr_engine()));
            // El vigía empieza sin vigilar nada: cada pasada le dice qué
            // bibliotecas mirar, empezando por la del arranque.
            let handle = app.handle().clone();
            let vigia = Vigia::empezar(vigilar::PAUSA, move || por_cambio(&handle))
                .map_err(|e| eprintln!("no se puede vigilar tus carpetas: {e}"))
                .ok();
            app.manage(Vigilante(Mutex::new(vigia)));
            app.manage(Pendiente(AtomicBool::new(false)));
            app.manage(EstadoRespaldo {
                seleccionada: Mutex::new(None),
                restaurando: AtomicBool::new(false),
            });
            if abierta {
                arrancar_en_fondo(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            vistazo,
            colecciones,
            coleccion,
            crear_coleccion,
            renombrar_coleccion,
            borrar_coleccion,
            meter_en_coleccion,
            sacar_de_coleccion,
            mover_entre_colecciones,
            lente,
            cielo,
            explorar,
            recientes,
            sensibles,
            vencimientos,
            corregir_vencimiento,
            papelera,
            olvidar,
            carpetas_marcadas,
            marcar_carpeta,
            entidad,
            previsualizar,
            abrir,
            revelar,
            corregir,
            confirmar,
            estado,
            seguridad,
            clave_recuperacion,
            recuperacion_guardada,
            recuperar,
            pin_desbloquear,
            pin_bloquear,
            pin_poner,
            pin_quitar,
            pin_restablecer,
            actualizar,
            anadir_biblioteca,
            respaldo_crear,
            respaldo_seleccionar,
            respaldo_comprobar,
            respaldo_ensayar,
            restauracion_planificar,
            restauracion_ejecutar,
            reanudar_reindexado,
            motivo_reindexado_pausado,
            abrir_carpeta_salvaguarda
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo arrancar la ventana");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct DirPrueba(PathBuf);

    impl DirPrueba {
        fn nuevo(prefijo: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "obrain-tauri-test-{}-{}-{}",
                prefijo,
                std::process::id(),
                nanos
            ));
            std::fs::create_dir_all(&dir).expect("crear directorio temporal de prueba");
            Self(dir)
        }
    }

    impl Drop for DirPrueba {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn restauracion_cancela_si_indexador_sigue_activo_y_preserva_cerrojo_y_brain() {
        let tmp = DirPrueba::nuevo("indexador-activo");
        let db = tmp.0.join("brain.obrain");
        let brain = Brain::open(&db).expect("abrir base sintética");

        let cerebro = Cerebro(Mutex::new(Some(brain)));
        let indexando = Indexando(AtomicBool::new(true));
        let vigilante = Vigilante(Mutex::new(None));
        let pendiente = Pendiente(AtomicBool::new(true));

        let err = tauri::async_runtime::block_on(coordinar_restauracion(
            tmp.0.join("respaldo-no-usado"),
            db.clone(),
            None,
            &cerebro,
            &indexando,
            &vigilante,
            &pendiente,
            Duration::from_millis(60),
        ))
        .expect_err("debe cancelar cuando el indexador no termina dentro del plazo");

        assert!(err.contains("indexador"), "mensaje inesperado: {err}");
        // El cerrojo sigue perteneciendo al indexador en curso (no se limpia en falso)
        assert!(indexando.0.load(Ordering::SeqCst));
        assert!(pendiente.0.load(Ordering::SeqCst));
        // Brain nunca se cerró ni perdió su referencia
        assert!(cerebro.0.lock().unwrap().is_some());
    }

    #[test]
    fn fallo_previo_a_sustitucion_recupera_brain_y_libera_cerrojo() {
        let tmp = DirPrueba::nuevo("fallo-previo");
        let db = tmp.0.join("brain.obrain");
        let mut brain = Brain::open(&db).expect("abrir base sintética");
        brain
            .create_collection("Importante", "test")
            .expect("crear colección");

        let cerebro = Cerebro(Mutex::new(Some(brain)));
        let indexando = Indexando(AtomicBool::new(false));
        let vigilante = Vigilante(Mutex::new(None));
        let pendiente = Pendiente(AtomicBool::new(false));

        let err = tauri::async_runtime::block_on(coordinar_restauracion(
            tmp.0.join("respaldo-inexistente"),
            db.clone(),
            None,
            &cerebro,
            &indexando,
            &vigilante,
            &pendiente,
            Duration::from_secs(1),
        ))
        .expect_err("debe fallar ante respaldo inexistente");

        assert!(!err.is_empty());
        assert!(!indexando.0.load(Ordering::SeqCst));

        let guarda = cerebro.0.lock().unwrap();
        let brain_reabierto = guarda.as_ref().expect("Brain debe reabrirse tras fallo previo");
        assert_eq!(brain_reabierto.collections().unwrap().len(), 1);
    }

    #[test]
    fn transicion_ambigua_detiene_apertura_preserva_evidencias_y_no_crea_base_vacia() {
        let tmp = DirPrueba::nuevo("transicion-ambigua");
        let db = tmp.0.join("brain.obrain");
        let brain = Brain::open(&db).expect("abrir base sintética");
        let carpeta_respaldo = tmp.0.join("respaldo-valido");
        respaldo::crear(&db, &carpeta_respaldo).expect("crear respaldo válido");

        // Inyectar recibo de transición con hashes inconclusos para provocar TransicionAmbigua
        let recibo_path = tmp.0.join(respaldo::RECIBO_TRANSICION_ARCHIVO);
        let recibo_json = format!(
            r#"{{
                "version": 1,
                "creado_en_ms": 1,
                "salvaguarda_dir": null,
                "staging_dir": "{}",
                "target_db": "{}",
                "target_llave": "{}",
                "staged_db_hash": "hash_staged_inexistente",
                "staged_llave_hash": "hash_staged_llave_inexistente",
                "target_db_hash_anterior": "hash_anterior_inexistente",
                "target_llave_hash_anterior": "hash_anterior_llave_inexistente",
                "estado": "sustituyendo"
            }}"#,
            tmp.0.join("staging").display().to_string().replace('\\', "\\\\"),
            db.display().to_string().replace('\\', "\\\\"),
            tmp.0.join("brain.obrain.llave").display().to_string().replace('\\', "\\\\"),
        );
        std::fs::write(&recibo_path, recibo_json.as_bytes()).expect("escribir recibo ambiguo");

        let cerebro = Cerebro(Mutex::new(Some(brain)));
        let indexando = Indexando(AtomicBool::new(false));
        let vigilante = Vigilante(Mutex::new(None));
        let pendiente = Pendiente(AtomicBool::new(false));

        let err = tauri::async_runtime::block_on(coordinar_restauracion(
            carpeta_respaldo,
            db.clone(),
            None,
            &cerebro,
            &indexando,
            &vigilante,
            &pendiente,
            Duration::from_secs(1),
        ))
        .expect_err("debe detenerse ante transición ambigua");

        assert!(err.contains("ambiguo"), "mensaje inesperado: {err}");
        assert!(!indexando.0.load(Ordering::SeqCst));
        // Brain permanece cerrado para no operar sobre un estado ambiguo
        assert!(cerebro.0.lock().unwrap().is_none());
        // El recibo de evidencia se conserva intacto en disco
        assert!(recibo_path.exists());
    }

    #[test]
    fn restauracion_coordinada_completa_sin_esperas_arbitrarias_y_pausa_reindexado() {
        let tmp = DirPrueba::nuevo("restauracion-ok");
        let db = tmp.0.join("brain.obrain");
        let mut brain = Brain::open(&db).expect("abrir base sintética");
        brain
            .create_collection("EnRespaldo", "test")
            .expect("crear colección respaldada");

        let carpeta_respaldo = tmp.0.join("respaldo-1");
        respaldo::crear(&db, &carpeta_respaldo).expect("crear respaldo");

        brain
            .create_collection("PosteriorDescartada", "test")
            .expect("crear colección posterior");
        assert_eq!(brain.collections().unwrap().len(), 2);

        let cerebro = Cerebro(Mutex::new(Some(brain)));
        let indexando = Indexando(AtomicBool::new(false));
        let vigilante = Vigilante(Mutex::new(None));
        let pendiente = Pendiente(AtomicBool::new(false));

        let informe = tauri::async_runtime::block_on(coordinar_restauracion(
            carpeta_respaldo,
            db.clone(),
            None,
            &cerebro,
            &indexando,
            &vigilante,
            &pendiente,
            Duration::from_secs(1),
        ))
        .expect("restauración coordinada satisfactoria");

        assert!(informe.reindexado_en_pausa);
        assert!(informe.salvaguarda_creada.as_ref().is_some_and(|p| p.exists()));
        assert!(!indexando.0.load(Ordering::SeqCst));

        let mut guarda = cerebro.0.lock().unwrap();
        let brain_restaurado = guarda.as_mut().expect("Brain debe quedar abierto tras restaurar");
        assert_eq!(brain_restaurado.collections().unwrap().len(), 1);
        assert!(brain_restaurado.security().unwrap().reindexado_pausado);
    }

    #[test]
    fn cerrada_o_en_pausa_inhibe_vigia_durante_pausa_y_se_reactiva_al_reanudar() {
        let tmp = DirPrueba::nuevo("vigia-pausa");
        let db = tmp.0.join("brain.obrain");
        let brain = Brain::open(&db).expect("abrir base sintética");
        let cerebro = Cerebro(Mutex::new(Some(brain)));

        assert!(!cerrada_o_en_pausa(&cerebro));

        {
            let guarda = cerebro.0.lock().unwrap();
            let b = guarda.as_ref().unwrap();
            seguridad::pausar_reindexado(b.store().conn(), "restauración de respaldo reciente")
                .unwrap();
        }
        assert!(cerrada_o_en_pausa(&cerebro));

        {
            let guarda = cerebro.0.lock().unwrap();
            let b = guarda.as_ref().unwrap();
            seguridad::reanudar_reindexado(b.store().conn()).unwrap();
        }
        assert!(!cerrada_o_en_pausa(&cerebro));

        *cerebro.0.lock().unwrap() = None;
        assert!(cerrada_o_en_pausa(&cerebro));
    }
}
