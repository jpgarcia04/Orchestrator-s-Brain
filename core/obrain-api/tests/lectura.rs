//! La apertura de la puerta de Aetheris (ADR-0038, ADR-0039 condición 3):
//! lee la base tal como está, sin crear, cambiar ni recuperar ningún archivo,
//! y dice con un `no_disponible` por qué no puede cuando no puede.
//!
//! Sólo bases sintéticas en carpetas temporales; ninguna prueba abre la
//! bóveda real.

use obrain_api::Brain;
use obrain_api::lectura::{TABLAS_DE_LA_PUERTA, abrir_para_leer};
use obrain_api::respaldo::{self, OpcionesRestauracion, RECIBO_TRANSICION_ARCHIVO, restaurar};
use obrain_api::seguridad::abrir_con;
use obrain_core::rusqlite::Connection;
use obrain_core::{Fallo, Method, NewAssertion, NoDisponible, Store, schema};
use obrain_keys::KdfParams;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const K: KdfParams = KdfParams::FAST_FOR_TESTS;
const AYUDANTE: &str = "OBRAIN_LECTURA_AYUDANTE";

struct Carpeta(PathBuf);

impl Carpeta {
    fn nueva(nombre: &str) -> Self {
        let ruta = std::env::temp_dir().join(format!(
            "obrain-lectura-{nombre}-{}-{}",
            std::process::id(),
            obrain_core::EntityId::new()
        ));
        std::fs::create_dir_all(&ruta).unwrap();
        Self(ruta)
    }
    fn db(&self) -> PathBuf {
        self.0.join("brain.obrain")
    }
}

impl Drop for Carpeta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sufijo(p: &Path, s: &str) -> PathBuf {
    let mut o = p.as_os_str().to_owned();
    o.push(s);
    o.into()
}

/// Una base cifrada como la deja la ventana, con `n` aserciones, y cerrada.
fn base(db: &Path, n: usize) {
    let mut store = abrir_con(db, K).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    escribir(&store, n, 1_700_000_000_000);
    let brain = Brain::try_from_store(store).unwrap();
    brain.recovery_code().unwrap();
    brain.confirm_recovery_saved().unwrap();
}

fn escribir(store: &Store, n: usize, at: i64) {
    for i in 0..n {
        let e = store.create_entity(at + i as i64).unwrap();
        store
            .assert(NewAssertion::new(
                e,
                "about",
                format!("tema sintético {i}"),
                Method::Deterministic,
                at + i as i64,
            ))
            .unwrap();
    }
}

/// Todos los archivos de la carpeta con sus bytes, menos los que se piden
/// excluir. Que aparezca o desaparezca uno también cuenta.
fn foto(dir: &Path, sin: &[&str]) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !sin.iter().any(|s| n.ends_with(s)))
        .map(|n| {
            let b = std::fs::read(dir.join(&n)).unwrap();
            (n, b)
        })
        .collect();
    v.sort();
    v
}

fn nombres(dir: &Path) -> Vec<String> {
    foto(dir, &[]).into_iter().map(|(n, _)| n).collect()
}

/// Las aserciones sintéticas «about» que se ven desde esta lectura.
fn aserciones(l: &obrain_core::Lectura) -> i64 {
    l.leer(|s| {
        Ok(s.conn().query_row(
            "SELECT count(*) FROM assertions WHERE predicate = 'about'",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}

fn motivo(r: Result<obrain_core::Lectura, NoDisponible>) -> &'static str {
    r.map(|_| "abierta").unwrap_or_else(|n| n.motivo())
}

/// Con la llave sabida, sin pasar por nada del producto: para preparar casos
/// que el producto no deja preparar (una versión vieja, una tabla quitada).
fn a_mano(db: &Path, sql: &str) {
    let key = obrain_keys::unlock(db).unwrap();
    let c = Connection::open(db).unwrap();
    c.execute_batch(&key.sqlcipher_pragma()).unwrap();
    c.execute_batch(sql).unwrap();
}

// ───────────────────────────────────────────────────── el proceso de al lado

/// La ventana, en pequeño: otro proceso que tiene la base abierta y acaba de
/// escribir en ella (lo escrito sigue en el log). Se mata y se espera al
/// soltarlo, pase lo que pase en la prueba.
struct Ventana(Child);

impl Ventana {
    fn abrir(db: &Path) -> Self {
        let mut hijo = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ayudante_que_tiene_la_base_abierta",
                "--ignored",
                "--nocapture",
            ])
            .env(AYUDANTE, db)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut linea = String::new();
        let mut salida = BufReader::new(hijo.stdout.take().unwrap());
        while !linea.contains("listo") {
            linea.clear();
            assert!(
                salida.read_line(&mut linea).unwrap() > 0,
                "el ayudante terminó sin abrir"
            );
        }
        Self(hijo)
    }
}

impl Drop for Ventana {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "lo arranca Ventana::abrir en otro proceso"]
fn ayudante_que_tiene_la_base_abierta() {
    let Ok(db) = std::env::var(AYUDANTE) else {
        return;
    };
    let store = abrir_con(Path::new(&db), K).unwrap();
    store
        .conn()
        .execute_batch("PRAGMA wal_autocheckpoint = 0;")
        .unwrap();
    escribir(&store, 2, 1_800_000_000_000);
    println!("listo");
    std::io::stdout().flush().unwrap();
    // Hasta que la prueba suelte la tubería.
    let _ = std::io::stdin().read_to_end(&mut Vec::new());
}

// ─────────────────────────────────────────────────────────────── las pruebas

#[test]
fn sola_se_lee_sin_crear_ni_cambiar_ningun_archivo() {
    let dir = Carpeta::nueva("sola");
    base(&dir.db(), 3);
    assert_eq!(nombres(&dir.0), ["brain.obrain", "brain.obrain.llave"]);
    let antes = foto(&dir.0, &[]);

    let l = abrir_para_leer(&dir.db()).unwrap();
    assert!(!l.con_log());
    assert_eq!(aserciones(&l), 3);
    drop(l);

    assert_eq!(foto(&dir.0, &[]), antes, "ni un byte ni un archivo nuevo");
}

#[test]
fn con_la_ventana_abierta_lee_su_log_sin_tocar_la_base_ni_el_log() {
    let dir = Carpeta::nueva("ventana");
    base(&dir.db(), 3);
    let _ventana = Ventana::abrir(&dir.db());
    assert!(sufijo(&dir.db(), "-wal").exists() && sufijo(&dir.db(), "-shm").exists());
    let antes = foto(&dir.0, &[]);

    let l = abrir_para_leer(&dir.db()).unwrap();
    assert!(l.con_log());
    assert_eq!(
        aserciones(&l),
        5,
        "lo que la ventana escribió está en su log"
    );
    drop(l);

    // Desde otro proceso, `readonly_shm` abre también el índice del log en
    // sólo lectura: no cambia ni él.
    assert_eq!(foto(&dir.0, &[]), antes);
}

#[test]
fn el_log_de_un_corte_se_lee_sin_recuperarlo() {
    let dir = Carpeta::nueva("corte");
    let origen = Carpeta::nueva("corte-origen");
    base(&origen.db(), 3);
    {
        // Copiar base, log e índice mientras otra conexión los tiene: lo que
        // queda tras un corte de luz con la ventana abierta.
        let store = abrir_con(&origen.db(), K).unwrap();
        store
            .conn()
            .execute_batch("PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        escribir(&store, 2, 1_800_000_000_000);
        for s in ["", "-wal", "-shm", ".llave"] {
            std::fs::copy(sufijo(&origen.db(), s), sufijo(&dir.db(), s)).unwrap();
        }
    }
    let antes = foto(&dir.0, &[]);

    let l = abrir_para_leer(&dir.db()).unwrap();
    assert!(l.con_log());
    assert_eq!(aserciones(&l), 5);
    drop(l);

    assert_eq!(
        foto(&dir.0, &[]),
        antes,
        "no recupera el log ni reescribe el índice"
    );
}

#[test]
fn si_alguien_escribe_mientras_lee_sola_lo_leido_se_tira() {
    let dir = Carpeta::nueva("escribe");
    base(&dir.db(), 3);
    let l = abrir_para_leer(&dir.db()).unwrap();
    assert!(!l.con_log());

    let r = l.leer(|s| {
        let n: i64 = s
            .conn()
            .query_row("SELECT count(*) FROM assertions", [], |r| r.get(0))?;
        // La ventana abre y escribe a mitad de la lectura.
        let w = abrir_con(&dir.db(), K).unwrap();
        escribir(&w, 1, 1_800_000_000_000);
        Ok(n)
    });
    assert!(
        matches!(r, Err(Fallo::NoDisponible(NoDisponible::Ocupada(_)))),
        "{r:?}"
    );
}

#[test]
fn una_escritura_desde_la_lectura_es_un_error_y_no_escribe() {
    let dir = Carpeta::nueva("escritura");
    base(&dir.db(), 1);
    let antes = foto(&dir.0, &[]);
    let l = abrir_para_leer(&dir.db()).unwrap();
    let r = l.leer(|s| {
        s.conn()
            .execute("INSERT INTO seguridad (clave, valor) VALUES ('x', 'y')", [])?;
        Ok(())
    });
    assert!(matches!(r, Err(Fallo::Error(_))), "{r:?}");
    drop(l);
    assert_eq!(foto(&dir.0, &[]), antes);
}

#[test]
fn lo_que_necesita_recuperacion_o_esta_a_medias_es_ocupada_y_no_se_toca() {
    // Cada caso añade un archivo a una base sana.
    let casos: &[(&str, &str)] = &[
        ("-wal", "un log sin su índice"),
        ("-shm", "un índice sin su log"),
        ("-journal", "un journal caliente"),
        (".cifrando", "un cifrado a medias"),
        (".llave.tmp", "una llave a medio escribir"),
    ];
    for (s, que) in casos {
        let dir = Carpeta::nueva("ocupada");
        base(&dir.db(), 1);
        std::fs::write(sufijo(&dir.db(), s), b"resto sintetico").unwrap();
        let antes = foto(&dir.0, &[]);
        assert_eq!(motivo(abrir_para_leer(&dir.db())), "ocupada", "{que}");
        assert_eq!(foto(&dir.0, &[]), antes, "{que}");
    }

    let dir = Carpeta::nueva("recibo");
    base(&dir.db(), 1);
    std::fs::write(dir.0.join(RECIBO_TRANSICION_ARCHIVO), b"{}").unwrap();
    let antes = foto(&dir.0, &[]);
    assert_eq!(
        motivo(abrir_para_leer(&dir.db())),
        "ocupada",
        "una restauración a medias"
    );
    assert_eq!(
        foto(&dir.0, &[]),
        antes,
        "la puerta no repara la restauración"
    );
}

#[test]
fn sin_base_sin_llave_o_con_otra_llave_es_base_cerrada_y_no_crea_nada() {
    let dir = Carpeta::nueva("cerrada");
    assert_eq!(motivo(abrir_para_leer(&dir.db())), "base_cerrada");
    assert!(nombres(&dir.0).is_empty(), "no crea una base ni su llave");

    // Una base en claro: la cifra la ventana, la puerta no la lee.
    {
        let c = Connection::open(dir.db()).unwrap();
        c.execute_batch(schema::DDL).unwrap();
    }
    let antes = foto(&dir.0, &[]);
    assert_eq!(motivo(abrir_para_leer(&dir.db())), "base_cerrada");
    assert_eq!(foto(&dir.0, &[]), antes);

    // La llave de otra base.
    let a = Carpeta::nueva("cerrada-a");
    let b = Carpeta::nueva("cerrada-b");
    base(&a.db(), 1);
    base(&b.db(), 1);
    std::fs::copy(sufijo(&b.db(), ".llave"), sufijo(&a.db(), ".llave")).unwrap();
    let antes = foto(&a.0, &[]);
    assert_eq!(motivo(abrir_para_leer(&a.db())), "base_cerrada");
    assert_eq!(foto(&a.0, &[]), antes);
}

#[test]
fn otra_version_o_sin_las_tablas_de_la_puerta_es_version_distinta() {
    let casos = [
        format!("PRAGMA user_version = {};", schema::SCHEMA_VERSION - 1),
        format!("PRAGMA user_version = {};", schema::SCHEMA_VERSION + 1),
        "DROP TABLE vectors;".to_string(),
    ];
    for sql in &casos {
        let dir = Carpeta::nueva("version");
        base(&dir.db(), 1);
        a_mano(&dir.db(), sql);
        let antes = foto(&dir.0, &[]);
        assert_eq!(
            motivo(abrir_para_leer(&dir.db())),
            "version_distinta",
            "{sql}"
        );
        assert_eq!(foto(&dir.0, &[]), antes, "no migra: {sql}");
    }
    assert!(TABLAS_DE_LA_PUERTA.contains(&"vectors"));
}

#[test]
fn soltada_la_lectura_restaurar_un_respaldo_funciona() {
    let dir = Carpeta::nueva("restaurar");
    let fuente = dir.0.join("fuente").join("brain.obrain");
    std::fs::create_dir_all(fuente.parent().unwrap()).unwrap();
    base(&fuente, 4);
    let copia = dir.0.join("respaldo");
    respaldo::crear(&fuente, &copia).unwrap();

    let activa = dir.0.join("activa").join("brain.obrain");
    std::fs::create_dir_all(activa.parent().unwrap()).unwrap();
    base(&activa, 2);

    // Una llamada de la puerta: abre, lee, suelta.
    let l = abrir_para_leer(&activa).unwrap();
    assert_eq!(aserciones(&l), 2);
    drop(l);

    let opciones = OpcionesRestauracion {
        codigo_recuperacion: None,
        autorizar: true,
        pausar_reindexado: true,
        carpeta_salvaguarda: None,
    };
    let r = restaurar(&copia, &activa, opciones);
    assert!(r.is_ok(), "la puerta no deja la base «en uso»: {r:?}");
    assert_eq!(aserciones(&abrir_para_leer(&activa).unwrap()), 4);
}
