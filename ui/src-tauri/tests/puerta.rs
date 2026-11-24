//! El ejecutable de verdad en modo puerta (ADR-0039, paso 5): se lanza como
//! lo lanzaría Aetheris, con una base sintética y sin pantalla, y se le habla
//! MCP por stdin.
//!
//! Sin `DISPLAY` ni `WAYLAND_DISPLAY`: si el modo puerta intentara abrir la
//! ventana, Tauri no podría y el proceso fallaría. Con `HOME` y las carpetas
//! XDG en un temporal propio, nada de lo que haga sale de él. Sólo archivos
//! inventados; ninguna prueba abre la bóveda real.

use obrain_api::puerta::Posicion;
use obrain_api::{Brain, indexar, seguridad};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Temporal(PathBuf);

impl Drop for Temporal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// El proceso de la puerta. Si la prueba falla a medias, se mata y se espera.
struct Hijo(Child);

impl Drop for Hijo {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn temporal(nombre: &str) -> Temporal {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "obrain-app-puerta-{nombre}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Temporal(dir)
}

/// Una biblioteca sintética indexada, con la puerta en `posicion`.
fn base(raiz: &Path, posicion: Posicion) -> PathBuf {
    let biblioteca = raiz.join("Biblioteca Sintetica");
    std::fs::create_dir_all(biblioteca.join("Normal")).unwrap();
    std::fs::write(
        biblioteca.join("Normal").join("recibo.txt"),
        "Recibo sintético de la academia: pagado en azafrán.",
    )
    .unwrap();
    let db = raiz.join("datos").join("brain.obrain");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut store = seguridad::abrir(&db).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    let informe = indexar::index_with(&mut store, &[biblioteca], None, None).unwrap();
    assert!(informe.errors.is_empty(), "{informe:?}");
    let mut brain = Brain::try_from_store(store).unwrap();
    brain.set_gate_position(posicion).unwrap();
    drop(brain);
    db
}

/// Lanza `obrain-app` con `argumentos`, le escribe `peticiones`, cierra su
/// entrada y espera a que salga. Devuelve el código y las respuestas.
fn lanzar(
    raiz: &Path,
    db: &Path,
    argumentos: &[&str],
    peticiones: &[Value],
) -> (Option<i32>, Vec<Value>) {
    let casa = raiz.join("casa");
    std::fs::create_dir_all(&casa).unwrap();
    let mut hijo = Hijo(
        Command::new(env!("CARGO_BIN_EXE_obrain-app"))
            .args(argumentos)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &casa)
            .env("XDG_DATA_HOME", casa.join("datos"))
            .env("XDG_CONFIG_HOME", casa.join("config"))
            .env("XDG_CACHE_HOME", casa.join("cache"))
            .env("XDG_RUNTIME_DIR", casa.join("runtime"))
            .env("OBRAIN_DB", db)
            .envs(std::env::var_os("LD_LIBRARY_PATH").map(|v| ("LD_LIBRARY_PATH", v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut entrada = hijo.0.stdin.take().unwrap();
    for p in peticiones {
        entrada.write_all(p.to_string().as_bytes()).unwrap();
        entrada.write_all(b"\n").unwrap();
    }
    drop(entrada);
    let mut salida = String::new();
    hijo.0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut salida)
        .unwrap();
    let limite = Instant::now() + Duration::from_secs(60);
    let codigo = loop {
        if let Some(estado) = hijo.0.try_wait().unwrap() {
            break estado.code();
        }
        assert!(
            Instant::now() < limite,
            "la puerta no salió al cerrarle la entrada"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let respuestas = salida
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (codigo, respuestas)
}

fn inicio() -> Value {
    json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": { "name": "aetheris-de-prueba", "version": "0" } } })
}

fn localizar(id: u32) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": "localizar", "arguments": { "consulta": "azafrán" } } })
}

#[test]
fn el_ejecutable_en_modo_puerta_contesta_por_stdio_sin_ventana_y_sale_al_cerrar() {
    let t = temporal("contesta");
    let db = base(&t.0, Posicion::SoloLocalizar);
    let antes = std::fs::read(&db).unwrap();
    let (codigo, r) = lanzar(
        &t.0,
        &db,
        &["--puerta"],
        &[
            inicio(),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
            localizar(2),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                    "params": { "name": "leer", "arguments": { "consulta": "azafrán" } } }),
        ],
    );
    assert_eq!(codigo, Some(0), "{r:?}");
    assert_eq!(r.len(), 4, "{r:?}");
    assert_eq!(r[0]["result"]["serverInfo"]["name"], "obrain");
    assert_eq!(r[0]["result"]["serverInfo"]["version"], "1");
    let nombres: Vec<&str> = r[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["name"].as_str().unwrap())
        .collect();
    assert_eq!(nombres, ["localizar", "leer", "documento"]);
    let resultados = r[2]["result"]["structuredContent"]["resultados"]
        .as_array()
        .unwrap();
    assert_eq!(resultados.len(), 1, "{r:?}");
    assert_eq!(resultados[0]["ruta"], "Normal/recibo.txt");
    // En «sólo localizar», leer no.
    assert_eq!(
        r[3]["result"]["structuredContent"]["no_disponible"]["motivo"],
        "apagada"
    );
    // Leyó sin cambiar la base ni crear nada en la casa del proceso más que
    // lo que ya estaba.
    assert_eq!(std::fs::read(&db).unwrap(), antes);
    assert!(!t.0.join("casa").join("datos").exists());
}

#[test]
fn el_ejecutable_en_modo_puerta_con_la_puerta_apagada_dice_apagada() {
    let t = temporal("apagada");
    let db = base(&t.0, Posicion::Apagada);
    let (codigo, r) = lanzar(&t.0, &db, &["--puerta"], &[inicio(), localizar(1)]);
    assert_eq!(codigo, Some(0));
    assert_eq!(
        r[1]["result"]["structuredContent"]["no_disponible"]["motivo"],
        "apagada"
    );
}

#[test]
fn el_modo_puerta_no_admite_mas_argumentos() {
    let t = temporal("argumentos");
    let db = t.0.join("no-existe.obrain");
    let (codigo, r) = lanzar(&t.0, &db, &["--puerta", "--otra"], &[inicio()]);
    assert_eq!(codigo, Some(2));
    assert!(r.is_empty());
    assert!(!db.exists());
}
