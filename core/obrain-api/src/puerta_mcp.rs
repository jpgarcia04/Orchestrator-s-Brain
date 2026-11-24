//! **El programa de la puerta de Aetheris**: MCP por stdio, sin ventana
//! (ADR-0038, B1 y B9; ADR-0039, paso 5).
//!
//! Lo arranca el cliente como proceso hijo (`obrain-app --puerta`) y vive
//! mientras el cliente le tenga abierta la entrada. Habla JSON-RPC 2.0, un
//! mensaje por línea, y sólo entiende `initialize`, `ping`, `tools/list` y
//! `tools/call`, con las tres herramientas de [`crate::puerta`]. No hay
//! recursos, ni muestreo, ni puerto, ni red.
//!
//! Lo que lo hace seguro no está aquí, sino debajo, y aquí sólo se respeta:
//!
//! - **Cada llamada abre la base para leer y la suelta** al contestar
//!   ([`crate::lectura::abrir_para_leer`]), así que entre llamadas no tiene
//!   nada abierto (ADR-0038, ajuste 9) y nunca escribe. Lo que la apertura
//!   no deja leer sale como `no_disponible` con su motivo.
//! - **Qué sale lo decide el núcleo**: las herramientas de [`crate::puerta`]
//!   leen el ajuste y aplican `SoloNormal` dentro de la misma lectura. Esto
//!   sólo traduce sus respuestas a MCP.
//! - **La base es la de `OBRAIN_DB` o la de la carpeta fija**
//!   ([`base_de_la_puerta`]), nunca [`obrain_core::ubicacion::carpeta`], que
//!   puede mudar datos: la puerta no mueve nada.
//! - **Lo que entra es de quien llama**: cada argumento se comprueba contra el
//!   `inputSchema` que se publica, una línea demasiado larga se descarta sin
//!   leerla entera, y un fallo se contesta sin detalles, porque un mensaje de
//!   error puede llevar la ruta de la base y con ella el usuario de Windows.

use crate::puerta::{self, NoSale};
use obrain_core::{Fallo, TimePoint};
use obrain_embed::embedder::Embedder;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// La versión del contrato de la puerta (ADR-0038, ajuste 6). Es la que va en
/// `serverInfo.version` y sólo cambia cuando cambia la forma de una
/// herramienta, no con cada versión de la aplicación.
pub const VERSION_DEL_CONTRATO: &str = "1";

/// El nombre con el que se presenta, el que Aetheris comprueba.
pub const NOMBRE: &str = "obrain";

/// Las versiones de MCP que entiende, de la más nueva a la más vieja. Todas
/// dicen lo mismo para lo poco que la puerta usa.
const PROTOCOLOS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Una línea más larga que esto no es una petición de esta puerta: se
/// descarta sin guardarla entera en memoria.
pub const LINEA_MAXIMA: usize = 64 * 1024;

/// Lo más largo que puede ser una consulta, en caracteres.
pub const CONSULTA_MAXIMA: usize = 500;

/// Lo más largo que puede ser un identificador de documento.
const ID_MAXIMO: usize = 64;

/// Cuántos resultados pide una llamada que no dice cuántos.
const LIMITE_POR_DEFECTO: usize = 10;

/// Lo que se contesta cuando algo falla por debajo. Sin detalles a propósito.
const FALLO: &str = "Brain no pudo leer la base.";

/// La base que abre la puerta: `OBRAIN_DB` si está puesta, si no la de la
/// carpeta fija de obrain. **Nunca** [`obrain_core::ubicacion::carpeta`], que
/// la primera vez muda la base vieja: si la base sigue en la carpeta vieja,
/// la puerta no la encuentra y lo dice, y la mudanza la hace la ventana.
pub fn base_de_la_puerta() -> PathBuf {
    match std::env::var_os("OBRAIN_DB") {
        Some(p) => PathBuf::from(p),
        None => obrain_core::ubicacion::fija().join(obrain_core::ubicacion::BASE),
    }
}

/// Carga el modelo de significado la primera vez que hace falta.
pub type Cargador = Box<dyn FnMut() -> Option<Box<dyn Embedder>>>;

/// Una puerta: la base que abre y el modelo, que no es la base y por eso se
/// puede quedar cargado entre llamadas.
pub struct Puerta {
    db: PathBuf,
    cargar: Cargador,
    modelo: Option<Option<Box<dyn Embedder>>>,
    iniciada: bool,
}

impl Puerta {
    /// La puerta de verdad: el modelo de la carpeta de modelos, si está.
    pub fn nueva(db: PathBuf) -> Self {
        Self::con_modelo(
            db,
            Box::new(|| {
                crate::load_model(None)
                    .ok()
                    .flatten()
                    .map(|e| e as Box<dyn Embedder>)
            }),
        )
    }

    /// Con otro modo de cargar el modelo (las pruebas).
    pub fn con_modelo(db: PathBuf, cargar: Cargador) -> Self {
        Self {
            db,
            cargar,
            modelo: None,
            iniciada: false,
        }
    }

    /// Atiende una línea y devuelve la respuesta, o nada si era una
    /// notificación (o una respuesta del cliente, que esta puerta no pidió).
    pub fn atender(&mut self, linea: &[u8]) -> Option<Value> {
        let Ok(mensaje) = serde_json::from_slice::<Value>(linea) else {
            return Some(error(Value::Null, -32700, "no es JSON"));
        };
        let Some(obj) = mensaje.as_object() else {
            // Ni lotes (MCP 2025-06-18 los quitó) ni otra cosa.
            return Some(error(Value::Null, -32600, "petición no válida"));
        };
        let id = obj.get("id").cloned();
        let valido = obj.get("jsonrpc").and_then(Value::as_str) == Some("2.0");
        let Some(metodo) = obj.get("method").and_then(Value::as_str) else {
            // Una respuesta del cliente no se contesta; lo demás, error.
            return match id {
                Some(_) if obj.contains_key("result") || obj.contains_key("error") => None,
                Some(id) if id_valido(&id) => Some(error(id, -32600, "petición no válida")),
                _ => Some(error(Value::Null, -32600, "petición no válida")),
            };
        };
        let Some(id) = id else {
            // Una notificación nunca se contesta, se entienda o no.
            return None;
        };
        if !valido || !id_valido(&id) {
            let id = if id_valido(&id) { id } else { Value::Null };
            return Some(error(id, -32600, "petición no válida"));
        }
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        if !(params.is_null() || params.is_object()) {
            return Some(error(id, -32602, "los parámetros van en un objeto"));
        }
        Some(match metodo {
            "initialize" => {
                self.iniciada = true;
                exito(id, inicio(&params))
            }
            "ping" => exito(id, json!({})),
            "tools/list" if self.iniciada => exito(id, json!({ "tools": herramientas() })),
            "tools/call" if self.iniciada => match self.llamar(&params) {
                Ok(r) => exito(id, r),
                Err(mensaje) => error(id, -32602, &mensaje),
            },
            "tools/list" | "tools/call" => error(id, -32002, "falta initialize"),
            _ => error(id, -32601, "método desconocido"),
        })
    }

    /// `tools/call`: comprueba los argumentos, abre, lee y suelta.
    fn llamar(&mut self, params: &Value) -> Result<Value, String> {
        let nombre = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or("falta el nombre de la herramienta")?;
        let vacio = serde_json::Map::new();
        let args = match params.get("arguments") {
            None | Some(Value::Null) => &vacio,
            Some(Value::Object(a)) => a,
            Some(_) => return Err("los argumentos van en un objeto".into()),
        };
        let pedido = Pedido::de(nombre, args)?;
        let Puerta {
            db, cargar, modelo, ..
        } = self;
        let modelo = match pedido {
            Pedido::Localizar { .. } | Pedido::Leer { .. } => cargado(cargar, modelo),
            Pedido::Documento { .. } => None,
        };
        Ok(contestar(db, &pedido, modelo))
    }
}

/// El modelo, cargado la primera vez que se pide. Si no está o no carga, la
/// búsqueda es sólo léxica y la respuesta lo dice (`semantica: false`).
fn cargado<'a>(
    cargar: &mut Cargador,
    modelo: &'a mut Option<Option<Box<dyn Embedder>>>,
) -> Option<&'a dyn Embedder> {
    modelo.get_or_insert_with(cargar).as_deref()
}

/// Lo que se pidió, ya comprobado.
#[derive(Debug, Clone, PartialEq)]
enum Pedido {
    Localizar { consulta: String, limite: usize },
    Leer { consulta: String, limite: usize },
    Documento { id: String },
}

impl Pedido {
    /// Comprueba los argumentos contra el `inputSchema` publicado: nada que
    /// no esté en él, cada campo de su tipo y dentro de sus límites.
    fn de(nombre: &str, args: &serde_json::Map<String, Value>) -> Result<Self, String> {
        let permitidos: &[&str] = match nombre {
            "localizar" | "leer" => &["consulta", "limite"],
            "documento" => &["id"],
            _ => return Err(format!("herramienta desconocida: {}", recortar(nombre))),
        };
        if let Some(otro) = args.keys().find(|k| !permitidos.contains(&k.as_str())) {
            return Err(format!("argumento desconocido: {}", recortar(otro)));
        }
        if nombre == "documento" {
            let id = texto(args, "id", ID_MAXIMO)?;
            return Ok(Pedido::Documento { id });
        }
        let consulta = texto(args, "consulta", CONSULTA_MAXIMA)?;
        let limite = match args.get("limite") {
            None => LIMITE_POR_DEFECTO,
            Some(v) => v
                .as_u64()
                .filter(|n| (1..=puerta::LIMITE_MAXIMO as u64).contains(n))
                .ok_or(format!(
                    "limite tiene que ser un entero de 1 a {}",
                    puerta::LIMITE_MAXIMO
                ))? as usize,
        };
        Ok(if nombre == "leer" {
            Pedido::Leer { consulta, limite }
        } else {
            Pedido::Localizar { consulta, limite }
        })
    }
}

/// Un texto obligatorio, con algo más que espacios y no más largo que
/// `maximo` caracteres.
fn texto(
    args: &serde_json::Map<String, Value>,
    campo: &str,
    maximo: usize,
) -> Result<String, String> {
    let v = args
        .get(campo)
        .ok_or(format!("falta {campo}"))?
        .as_str()
        .ok_or(format!("{campo} tiene que ser texto"))?;
    if v.trim().is_empty() {
        return Err(format!("{campo} está vacío"));
    }
    if v.chars().count() > maximo {
        return Err(format!("{campo} pasa de {maximo} caracteres"));
    }
    Ok(v.to_string())
}

/// Lo que se repite de algo que mandó el cliente, cortado.
fn recortar(s: &str) -> String {
    s.chars().take(40).collect()
}

/// Un identificador de JSON-RPC: texto o entero. `null` no sirve.
fn id_valido(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

fn ahora() -> TimePoint {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    TimePoint::at(ms)
}

/// Abre la base para leer, contesta lo pedido y la suelta.
fn contestar(db: &Path, pedido: &Pedido, modelo: Option<&dyn Embedder>) -> Value {
    let lectura = match crate::lectura::abrir_para_leer(db) {
        Ok(l) => l,
        Err(no) => return no_disponible(no.motivo()),
    };
    let at = ahora();
    let mut fuera: Option<crate::Result<Result<Value, NoSale>>> = None;
    let leido = lectura.leer(|s| {
        fuera = Some(match pedido {
            Pedido::Localizar { consulta, limite } => {
                puerta::localizar(s, modelo, consulta, *limite, at).map(|r| r.map(a_json))
            }
            Pedido::Leer { consulta, limite } => {
                puerta::leer(s, modelo, consulta, *limite, at).map(|r| r.map(a_json))
            }
            Pedido::Documento { id } => puerta::documento(s, id, at).map(|r| r.map(a_json)),
        });
        Ok(())
    });
    drop(lectura);
    match (leido, fuera) {
        (Ok(()), Some(Ok(Ok(v)))) => respuesta(v),
        (Ok(()), Some(Ok(Err(no)))) => no_disponible(no.motivo()),
        (Err(Fallo::NoDisponible(no)), _) => no_disponible(no.motivo()),
        _ => fallo(),
    }
}

fn a_json<T: serde::Serialize>(t: T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

/// Una respuesta de herramienta: el JSON como texto, para cualquier cliente,
/// y como `structuredContent`, para el que lo lea.
fn respuesta(v: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": v.to_string() }],
        "structuredContent": v,
        "isError": false,
    })
}

/// `no_disponible` con su motivo (ADR-0038, B9): un estado, no un error.
fn no_disponible(motivo: &str) -> Value {
    respuesta(json!({ "no_disponible": { "motivo": motivo } }))
}

fn fallo() -> Value {
    json!({
        "content": [{ "type": "text", "text": FALLO }],
        "isError": true,
    })
}

fn exito(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// La respuesta a `initialize`: la versión de MCP que pidió el cliente si la
/// entiende, si no la más nueva que entiende.
fn inicio(params: &Value) -> Value {
    let pedida = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOLOS
        .iter()
        .find(|v| Some(**v) == pedida)
        .unwrap_or(&PROTOCOLOS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": NOMBRE,
            "title": "Orchestrator's Brain",
            "version": VERSION_DEL_CONTRATO,
        },
        "instructions": "Sólo lectura de lo normal ya clasificado en Brain. \
            Lo sensible, lo de bóveda y lo sin clasificar no se consultan.",
        "_meta": { "dev.obrain/aplicacion": env!("CARGO_PKG_VERSION") },
    })
}

/// Las tres herramientas, con el esquema que [`Pedido::de`] hace cumplir.
pub fn herramientas() -> Value {
    let consulta = json!({
        "type": "string",
        "minLength": 1,
        "maxLength": CONSULTA_MAXIMA,
        "description": "Lo que se busca, con palabras o por lo que significa.",
    });
    let limite = json!({
        "type": "integer",
        "minimum": 1,
        "maximum": puerta::LIMITE_MAXIMO,
        "default": LIMITE_POR_DEFECTO,
        "description": "Cuántos documentos como mucho.",
    });
    let anotaciones = json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false,
    });
    let busqueda = json!({
        "type": "object",
        "properties": { "consulta": consulta, "limite": limite },
        "required": ["consulta"],
        "additionalProperties": false,
    });
    json!([
        {
            "name": "localizar",
            "title": "Localizar en Brain",
            "description": "Nombres y rutas, relativas a su biblioteca, de los documentos normales \
                que tienen que ver con la consulta. No trae ningún texto de dentro. Con la puerta \
                apagada responde no_disponible, motivo apagada.",
            "inputSchema": busqueda,
            "annotations": anotaciones,
        },
        {
            "name": "leer",
            "title": "Leer de Brain",
            "description": "Pasajes literales de los documentos normales que tienen que ver con la \
                consulta, cada uno con su documento, su método (bm25 o knn) y dos fechas. Con la \
                puerta apagada o en «sólo localizar» responde no_disponible, motivo apagada.",
            "inputSchema": busqueda,
            "annotations": anotaciones,
        },
        {
            "name": "documento",
            "title": "Ficha de un documento",
            "description": "La ficha de un documento normal por su identificador: ruta completa, \
                colecciones, caducidad con su procedencia y relaciones deterministas. Para todo \
                lo demás responde no_disponible, motivo fuera_de_la_puerta. Con la puerta apagada \
                o en «sólo localizar», motivo apagada.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": ID_MAXIMO,
                        "description": "El identificador que dio localizar o leer.",
                    }
                },
                "required": ["id"],
                "additionalProperties": false,
            },
            "annotations": anotaciones,
        },
    ])
}

/// Lee una línea de `entrada` sin pasar de [`LINEA_MAXIMA`]. `Ok(None)` al
/// cerrarse la entrada; `Ok(Some(Err(())))` si la línea era demasiado larga,
/// que se descarta hasta su salto de línea sin guardarla.
fn leer_linea(
    entrada: &mut impl BufRead,
    linea: &mut Vec<u8>,
) -> io::Result<Option<Result<(), ()>>> {
    linea.clear();
    let mut larga = false;
    loop {
        let disponible = match entrada.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if disponible.is_empty() {
            // Fin de la entrada: lo que quedó sin salto de línea también cuenta.
            return Ok(if linea.is_empty() && !larga {
                None
            } else if larga {
                Some(Err(()))
            } else {
                Some(Ok(()))
            });
        }
        let (trozo, fin) = match disponible.iter().position(|b| *b == b'\n') {
            Some(i) => (&disponible[..i], Some(i + 1)),
            None => (disponible, None),
        };
        if !larga {
            if linea.len() + trozo.len() > LINEA_MAXIMA {
                larga = true;
                linea.clear();
            } else {
                linea.extend_from_slice(trozo);
            }
        }
        let usado = fin.unwrap_or(disponible.len());
        entrada.consume(usado);
        if fin.is_some() {
            return Ok(Some(if larga { Err(()) } else { Ok(()) }));
        }
    }
}

/// Atiende peticiones hasta que se cierra la entrada.
pub fn servir(
    puerta: &mut Puerta,
    mut entrada: impl BufRead,
    mut salida: impl Write,
) -> io::Result<()> {
    let mut linea = Vec::new();
    while let Some(leida) = leer_linea(&mut entrada, &mut linea)? {
        let respuesta = match leida {
            Err(()) => Some(error(Value::Null, -32600, "mensaje demasiado largo")),
            Ok(()) => {
                let sin_espacios = linea.trim_ascii();
                if sin_espacios.is_empty() {
                    continue;
                }
                puerta.atender(sin_espacios)
            }
        };
        if let Some(r) = respuesta {
            serde_json::to_writer(&mut salida, &r)?;
            salida.write_all(b"\n")?;
            salida.flush()?;
        }
    }
    Ok(())
}

/// El modo puerta del ejecutable: sirve la base de [`base_de_la_puerta`] por
/// stdin y stdout. Devuelve el código de salida.
pub fn main() -> i32 {
    let mut puerta = Puerta::nueva(base_de_la_puerta());
    let entrada = io::stdin().lock();
    let salida = io::stdout().lock();
    match servir(&mut puerta, entrada, salida) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn una(linea: &str) -> Value {
        let mut p = Puerta::con_modelo(PathBuf::from("/no/existe/brain.obrain"), Box::new(|| None));
        p.atender(linea.as_bytes()).expect("esperaba respuesta")
    }

    #[test]
    fn lo_que_no_es_una_peticion_valida_se_contesta_con_su_error() {
        assert_eq!(una("{")["error"]["code"], -32700);
        assert_eq!(una("[]")["error"]["code"], -32600);
        assert_eq!(
            una(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#)["error"]["code"],
            -32600
        );
        assert_eq!(
            una(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#)["error"]["code"],
            -32600
        );
        assert_eq!(
            una(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#)["error"]["code"],
            -32600
        );
        assert_eq!(
            una(r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#)["id"],
            Value::Null
        );
        assert_eq!(
            una(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":[1]}"#)["error"]["code"],
            -32602
        );
        assert_eq!(
            una(r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#)["error"]["code"],
            -32601
        );
        assert_eq!(
            una(r#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#)["result"],
            json!({})
        );
    }

    #[test]
    fn las_notificaciones_y_las_respuestas_no_se_contestan() {
        let mut p = Puerta::con_modelo(PathBuf::from("x"), Box::new(|| None));
        for linea in [
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"localizar"}}"#,
            r#"{"jsonrpc":"2.0","id":7,"result":{}}"#,
        ] {
            assert_eq!(p.atender(linea.as_bytes()), None, "{linea}");
        }
    }

    #[test]
    fn sin_initialize_no_hay_herramientas() {
        assert_eq!(
            una(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)["error"]["code"],
            -32002
        );
    }

    #[test]
    fn initialize_da_la_version_del_contrato_y_la_de_mcp_que_entiende() {
        let r = una(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        );
        assert_eq!(r["result"]["serverInfo"]["name"], "obrain");
        assert_eq!(r["result"]["serverInfo"]["version"], "1");
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(
            r["result"]["capabilities"],
            json!({ "tools": { "listChanged": false } })
        );
        let r = una(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        );
        assert_eq!(r["result"]["protocolVersion"], PROTOCOLOS[0]);
    }

    #[test]
    fn los_argumentos_se_comprueban_contra_el_esquema_publicado() {
        let args = |v: Value| v.as_object().unwrap().clone();
        let bien = Pedido::de("localizar", &args(json!({ "consulta": "recibo" }))).unwrap();
        assert_eq!(
            bien,
            Pedido::Localizar {
                consulta: "recibo".into(),
                limite: LIMITE_POR_DEFECTO
            }
        );
        for (nombre, a) in [
            ("localizar", json!({})),
            ("localizar", json!({ "consulta": "   " })),
            ("localizar", json!({ "consulta": 3 })),
            (
                "localizar",
                json!({ "consulta": "x".repeat(CONSULTA_MAXIMA + 1) }),
            ),
            ("localizar", json!({ "consulta": "x", "limite": 0 })),
            ("localizar", json!({ "consulta": "x", "limite": 51 })),
            ("localizar", json!({ "consulta": "x", "limite": 2.5 })),
            ("localizar", json!({ "consulta": "x", "limite": "3" })),
            ("leer", json!({ "consulta": "x", "pin": "1234" })),
            ("documento", json!({ "id": "" })),
            ("documento", json!({ "id": "x".repeat(ID_MAXIMO + 1) })),
            ("documento", json!({ "id": "x", "consulta": "y" })),
            ("corregir", json!({})),
        ] {
            assert!(
                Pedido::de(nombre, &args(a.clone())).is_err(),
                "{nombre} {a}"
            );
        }
        assert!(Pedido::de("leer", &args(json!({ "consulta": "x", "limite": 50 }))).is_ok());
        // El esquema publicado dice lo mismo que la comprobación.
        let h = herramientas();
        let busqueda = &h[0]["inputSchema"];
        assert_eq!(busqueda["additionalProperties"], false);
        assert_eq!(
            busqueda["properties"]["limite"]["maximum"],
            puerta::LIMITE_MAXIMO
        );
        assert_eq!(
            busqueda["properties"]["consulta"]["maxLength"],
            CONSULTA_MAXIMA
        );
        assert_eq!(
            h[2]["inputSchema"]["properties"]["id"]["maxLength"],
            ID_MAXIMO
        );
    }

    #[test]
    fn tools_list_da_exactamente_las_tres_y_todas_de_solo_lectura() {
        let h = herramientas();
        let nombres: Vec<&str> = h
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                assert_eq!(t["annotations"]["readOnlyHint"], true);
                t["name"].as_str().unwrap()
            })
            .collect();
        assert_eq!(nombres, ["localizar", "leer", "documento"]);
    }

    #[test]
    fn una_linea_demasiado_larga_se_descarta_y_la_siguiente_se_atiende() {
        let mut entrada = Vec::new();
        entrada.extend(std::iter::repeat_n(b'x', LINEA_MAXIMA + 10));
        entrada.extend_from_slice(b"\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}");
        let mut salida = Vec::new();
        let mut p = Puerta::con_modelo(PathBuf::from("x"), Box::new(|| None));
        servir(&mut p, &entrada[..], &mut salida).unwrap();
        let lineas: Vec<Value> = salida
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).unwrap())
            .collect();
        assert_eq!(lineas.len(), 2);
        assert_eq!(lineas[0]["error"]["code"], -32600);
        assert_eq!(lineas[1]["id"], 2);
        assert_eq!(lineas[1]["result"], json!({}));
    }

    #[test]
    fn sin_base_dice_base_cerrada_y_no_la_crea() {
        let dir =
            std::env::temp_dir().join(format!("obrain-puerta-mcp-sin-base-{}", std::process::id()));
        let db = dir.join("brain.obrain");
        let mut p = Puerta::con_modelo(db.clone(), Box::new(|| None));
        p.atender(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        let r = p
            .atender(br#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"localizar","arguments":{"consulta":"x"}}}"#)
            .unwrap();
        assert_eq!(
            r["result"]["structuredContent"]["no_disponible"]["motivo"], "base_cerrada",
            "{r}"
        );
        assert!(!dir.exists(), "la puerta no crea nada");
    }
}
