//! La aplicación de escritorio. **Diez comandos y ni una decisión.**
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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use obrain_api::{Brain, EntityView, Glance, Lens, Listing, Preview, Sky};
use obrain_core::EntityId;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

struct Cerebro(Mutex<Brain>);

/// Dónde está la base.
///
/// `OBRAIN_DB` manda, para poder abrir la misma base que producen los CLI de
/// M1–M4 sin copiar nada. Si no está, se usa la del perfil del usuario.
fn ruta_de_la_base() -> PathBuf {
    if let Ok(p) = std::env::var("OBRAIN_DB") {
        return PathBuf::from(p);
    }
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("obrain").join("brain.obrain")
}

/// Lo que la barra enseña al abrir, antes de que nadie escriba nada.
#[derive(serde::Serialize)]
struct Estado {
    db: String,
    semantic: bool,
    entities: i64,
    assertions: i64,
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
fn estado(cerebro: State<'_, Cerebro>) -> Result<Estado, String> {
    let brain = cerebro.0.lock().map_err(texto)?;
    let store = brain.store();
    let entities: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))
        .map_err(texto)?;
    Ok(Estado {
        db: ruta_de_la_base().display().to_string(),
        semantic: brain.semantic_ready(),
        entities,
        assertions: store.log_len().map_err(texto)?,
    })
}

fn main() {
    tauri::Builder::default()
        // Sin permisos para el webview a propósito: las dos salidas se invocan
        // desde `abrir` y `revelar`, que son Rust y consultan el log.
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db = ruta_de_la_base();
            if let Some(dir) = db.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            app.manage(Cerebro(Mutex::new(Brain::open(&db)?)));

            // El modelo, fuera del cerrojo. Se paga la carga en este hilo y sólo
            // se entra al cerebro para dejarlo puesto — un instante. Si el
            // modelo no está descargado, esto no hace nada y la barra sigue
            // respondiendo en léxico.
            let handle = app.handle().clone();
            std::thread::spawn(move || match obrain_api::load_model(None) {
                Ok(Some(modelo)) => {
                    if let Ok(mut b) = handle.state::<Cerebro>().0.lock() {
                        b.set_embedder(modelo);
                    }
                }
                Ok(None) => eprintln!(
                    "sin modelo local: la busqueda va en lexico. \
                     `obrain-embed <db> --descargar-modelo` lo enciende."
                ),
                Err(e) => eprintln!("no se pudo cargar el modelo: {e}"),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            vistazo,
            lente,
            cielo,
            explorar,
            entidad,
            previsualizar,
            abrir,
            revelar,
            corregir,
            estado
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo arrancar la ventana");
}
