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
//! qué se puede abrir se toma en Rust, contra el log.
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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use obrain_api::{
    default_db_path, Brain, Embedder, EntityView, FolderMarked, Glance, IndexReport, Lens, Listing,
    MarkedFolder, Preview, Row, Sky,
};
use obrain_core::{EntityId, Sensitivity, Store};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

struct Cerebro(Mutex<Brain>);

/// ¿Hay un reindexado en marcha? Dos a la vez escribirían lo mismo dos veces.
struct Indexando(AtomicBool);

/// Lo que la barra enseña al abrir, antes de que nadie escriba nada.
#[derive(serde::Serialize)]
struct Estado {
    db: String,
    semantic: bool,
    entities: i64,
    assertions: i64,
    /// Las carpetas que el usuario eligió indexar. Vacío = bóveda sin empezar.
    libraries: Vec<String>,
    /// Hay un reindexado en curso.
    indexing: bool,
}

/// Lo que recibe la ventana con el evento `indexado`.
#[derive(serde::Serialize, Clone)]
struct Indexado {
    report: Option<IndexReport>,
    error: Option<String>,
}

fn texto<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn vistazo(query: String, limit: usize, cerebro: State<'_, Cerebro>) -> Result<Glance, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.glance(&query, limit).map_err(texto)
}

#[tauri::command]
fn lente(
    focus: Vec<String>,
    radius: u8,
    budget: usize,
    cerebro: State<'_, Cerebro>,
) -> Result<Lens, String> {
    let focus: Vec<EntityId> = focus.into_iter().map(EntityId).collect();
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.lens(&focus, radius, budget).map_err(texto)
}

/// El cielo: lo que se ve al abrir, antes de que nadie escriba nada.
#[tauri::command]
fn cielo(target: usize, cerebro: State<'_, Cerebro>) -> Result<Sky, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.sky(target).map_err(texto)
}

/// El explorador de ADR-0014: qué hay dentro de esto.
///
/// `foco` a `None` entra por las raíces, que es donde empieza quien abre el
/// explorador sin haber enfocado nada en la galaxia.
#[tauri::command]
fn explorar(foco: Option<String>, cerebro: State<'_, Cerebro>) -> Result<Listing, String> {
    let foco = foco.map(EntityId);
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.children(foco.as_ref()).map_err(texto)
}

/// Lo último que tocaste: el «Recientes» del inicio.
#[tauri::command]
fn recientes(limit: usize, cerebro: State<'_, Cerebro>) -> Result<Vec<Row>, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.recent(limit).map_err(texto)
}

/// Lo que la puerta retiene: la vista «Sensibles».
#[tauri::command]
fn sensibles(cerebro: State<'_, Cerebro>) -> Result<Vec<Row>, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.sensitive().map_err(texto)
}

/// Las carpetas que marcaste como sensibles, para la vista «Sensibles».
#[tauri::command]
fn carpetas_marcadas(cerebro: State<'_, Cerebro>) -> Result<Vec<MarkedFolder>, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.marked_folders().map_err(texto)
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
    let mut brain = cerebro.0.lock().map_err(texto)?;
    brain.mark_folder(&EntityId(carpeta), nivel).map_err(texto)
}

#[tauri::command]
fn entidad(id: String, cerebro: State<'_, Cerebro>) -> Result<EntityView, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.entity(&EntityId(id)).map_err(texto)
}

/// Qué se ve del archivo. La segunda mitad de M5b.
///
/// `consulta` sólo decide **dónde se abre** el texto —el tramo que explica lo
/// que se buscó— y nunca cuánto se puede leer. Lo que se puede leer lo decide
/// la guarda de M1 dentro del núcleo, y no la toca ningún argumento de aquí.
#[tauri::command]
fn previsualizar(
    id: String,
    consulta: Option<String>,
    cerebro: State<'_, Cerebro>,
) -> Result<Preview, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain.preview(&EntityId(id), consulta.as_deref()).map_err(texto)
}

/// La ruta de una entidad, según el log. **La pone el núcleo, no la ventana.**
fn ruta_de(cerebro: &State<'_, Cerebro>, id: String) -> Result<String, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain
        .entity(&EntityId(id))
        .map_err(texto)?
        .path
        .ok_or_else(|| "esta entidad no es un archivo con ruta".to_string())
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
fn abrir(id: String, app: tauri::AppHandle, cerebro: State<'_, Cerebro>) -> Result<(), String> {
    let ruta = ruta_de(&cerebro, id)?;
    app.opener().open_path(ruta, None::<&str>).map_err(texto)
}

/// Lo revela en el explorador de Windows: abre su carpeta y lo selecciona.
///
/// Nunca ejecuta el archivo, así que no hereda el primer coste de [`abrir`].
#[tauri::command]
fn revelar(id: String, app: tauri::AppHandle, cerebro: State<'_, Cerebro>) -> Result<(), String> {
    let ruta = ruta_de(&cerebro, id)?;
    app.opener().reveal_item_in_dir(ruta).map_err(texto)
}

/// La corrección de I5. Escribe una aserción del usuario, que gana para
/// siempre. Es el único comando que **escribe**, y sólo en el log.
#[tauri::command]
fn corregir(
    subject: String,
    predicate: String,
    object: String,
    cerebro: State<'_, Cerebro>,
) -> Result<String, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    brain
        .correct(
            &EntityId(subject),
            &predicate,
            obrain_core::Value::Text(object),
        )
        .map_err(texto)
}

#[tauri::command]
fn estado(cerebro: State<'_, Cerebro>, indexando: State<'_, Indexando>) -> Result<Estado, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    let store = brain.store();
    let entities: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))
        .map_err(texto)?;
    Ok(Estado {
        db: default_db_path().display().to_string(),
        semantic: brain.semantic_ready(),
        entities,
        assertions: store.log_len().map_err(texto)?,
        libraries: obrain_api::indexar::libraries(store).map_err(texto)?,
        indexing: indexando.0.load(Ordering::SeqCst),
    })
}

/// Vuelve a leer tus bibliotecas. **Vuelve al instante**: el trabajo va en un
/// hilo y el resultado llega con el evento `indexado`. Devuelve `false` si ya
/// había uno en marcha.
#[tauri::command]
fn actualizar(app: AppHandle, cerebro: State<'_, Cerebro>, indexando: State<'_, Indexando>) -> bool {
    if indexando.0.swap(true, Ordering::SeqCst) {
        return false;
    }
    // El modelo se presta: vectorizar con el cerebro cogido dejaría la barra
    // sin responder. Mientras tanto la búsqueda sigue, en léxico.
    let modelo = cerebro.0.lock().ok().and_then(|mut b| b.take_embedder());
    std::thread::spawn(move || indexar_en_fondo(&app, None, modelo));
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
    if app.state::<Indexando>().0.load(Ordering::SeqCst) {
        return Ok(false);
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
    if app.state::<Indexando>().0.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    let modelo = app
        .state::<Cerebro>()
        .0
        .lock()
        .ok()
        .and_then(|mut b| b.take_embedder());
    std::thread::spawn(move || indexar_en_fondo(&app, Some(vec![ruta]), modelo));
    Ok(true)
}

/// El indexado de verdad. Siempre en un hilo aparte, con su propia conexión.
///
/// `raices` a `None` vuelve a leer todas las bibliotecas; con carpetas, las
/// añade (y deja las demás como estaban).
fn indexar_en_fondo(
    app: &AppHandle,
    raices: Option<Vec<PathBuf>>,
    modelo: Option<Box<dyn Embedder + Send>>,
) {
    let res = Store::open(default_db_path()).map_err(texto).and_then(|mut store| {
        let m = modelo.as_deref().map(|m| m as &dyn Embedder);
        match &raices {
            Some(r) => obrain_api::indexar::index(&mut store, r, m),
            None => obrain_api::indexar::reindex(&mut store, m),
        }
        .map_err(texto)
    });
    // El modelo vuelve al cerebro pase lo que pase.
    if let Some(m) = modelo {
        if let Ok(mut b) = app.state::<Cerebro>().0.lock() {
            b.set_embedder(m);
        }
    }
    app.state::<Indexando>().0.store(false, Ordering::SeqCst);
    let aviso = match res {
        Ok(r) => Indexado { report: Some(r), error: None },
        Err(e) => {
            eprintln!("no se pudo actualizar: {e}");
            Indexado { report: None, error: Some(e) }
        }
    };
    let _ = app.emit("indexado", aviso);
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
            app.manage(Cerebro(Mutex::new(Brain::open(&db)?)));
            // Arranca marcado: el hilo de abajo reindexa en cuanto tenga modelo.
            app.manage(Indexando(AtomicBool::new(true)));

            // El modelo, fuera del cerrojo, y después **tus bibliotecas al
            // día**: lo que añadiste a la carpeta mientras la aplicación estaba
            // cerrada aparece sin pulsar nada. Si el modelo no está descargado,
            // se indexa igual y la barra sigue respondiendo en léxico.
            let handle = app.handle().clone();
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
                indexar_en_fondo(&handle, None, modelo);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            vistazo,
            lente,
            cielo,
            explorar,
            recientes,
            sensibles,
            carpetas_marcadas,
            marcar_carpeta,
            entidad,
            previsualizar,
            abrir,
            revelar,
            corregir,
            estado,
            actualizar,
            anadir_biblioteca
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo arrancar la ventana");
}
