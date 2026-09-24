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
    cerebro: State<'_, Cerebro>,
) -> Result<Preview, String> {
    con(&cerebro, |b| {
        b.preview_at(
            &EntityId(id),
            consulta.as_deref(),
            tramo.map(|[x, y]| (x, y)),
        )
    })
}

/// La ruta de una entidad, según el log. **La pone el núcleo, no la ventana**,
/// y sólo si se puede abrir: con PIN, lo sensible pide desbloquear antes.
fn ruta_para_abrir(cerebro: &State<'_, Cerebro>, id: String) -> Result<String, String> {
    con(cerebro, |b| {
        let id = EntityId(id);
        if !b.may_open(&id)? {
            return Err(obrain_api::Error::Bloqueado);
        }
        Ok(b.entity(&id)?.path)
    })?
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
    let ruta = ruta_para_abrir(&cerebro, id)?;
    app.opener().open_path(ruta, None::<&str>).map_err(texto)
}

/// Lo revela en el explorador de Windows: abre su carpeta y lo selecciona.
///
/// Nunca ejecuta el archivo, así que no hereda el primer coste de [`abrir`].
#[tauri::command]
fn revelar(id: String, app: tauri::AppHandle, cerebro: State<'_, Cerebro>) -> Result<(), String> {
    let ruta = ruta_para_abrir(&cerebro, id)?;
    app.opener().reveal_item_in_dir(ruta).map_err(texto)
}

/// La corrección de I5. Escribe una aserción del usuario, que gana para
/// siempre. Es el único comando que **escribe** en el log, además de marcar
/// una carpeta y de [`corregir_vencimiento`].
#[tauri::command]
fn corregir(
    subject: String,
    predicate: String,
    object: String,
    cerebro: State<'_, Cerebro>,
) -> Result<String, String> {
    con(&cerebro, |b| {
        b.correct(
            &EntityId(subject),
            &predicate,
            obrain_core::Value::Text(object),
        )
    })
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

// ───────────────────────────────────────────────────────────── indexar

/// Vuelve a leer tus bibliotecas. **Vuelve al instante**: el trabajo va en un
/// hilo y el resultado llega con el evento `indexado`. Devuelve `false` si ya
/// había uno en marcha (o si la base está cerrada).
#[tauri::command]
fn actualizar(
    app: AppHandle,
    cerebro: State<'_, Cerebro>,
    indexando: State<'_, Indexando>,
) -> bool {
    if cerebro.0.lock().map(|g| g.is_none()).unwrap_or(true) {
        return false;
    }
    if indexando.0.swap(true, Ordering::SeqCst) {
        return false;
    }
    // El modelo se presta: vectorizar con el cerebro cogido dejaría la barra
    // sin responder. Mientras tanto la búsqueda sigue, en léxico.
    let modelo = cerebro
        .0
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().and_then(Brain::take_embedder));
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
    if app.state::<Indexando>().0.load(Ordering::SeqCst) {
        return Ok(false);
    }
    if app
        .state::<Cerebro>()
        .0
        .lock()
        .map(|g| g.is_none())
        .unwrap_or(true)
    {
        return Err(CERRADA.to_string());
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
        .and_then(|mut g| g.as_mut().and_then(Brain::take_embedder));
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
    // Lo que cambió mientras tanto, en una pasada más.
    if app.state::<Pendiente>().0.swap(false, Ordering::SeqCst) {
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
fn por_cambio(app: &AppHandle) {
    if app
        .state::<Cerebro>()
        .0
        .lock()
        .map(|g| g.is_none())
        .unwrap_or(true)
    {
        return;
    }
    if app.state::<Indexando>().0.swap(true, Ordering::SeqCst) {
        app.state::<Pendiente>().0.store(true, Ordering::SeqCst);
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
                Err(e) => return Err(e.into()),
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
            anadir_biblioteca
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo arrancar la ventana");
}
