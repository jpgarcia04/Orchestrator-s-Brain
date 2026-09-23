//! El vigía con un sistema de archivos de verdad (ADR-0024): `inotify` aquí,
//! `ReadDirectoryChangesW` en Windows, por el mismo `notify`.
//!
//! Los tiempos son generosos a propósito: lo que se comprueba es **cuántas**
//! veces avisa y **si** avisa, no cuánto tarda el sistema en entregar un
//! evento.

use obrain_api::vigilar::Vigia;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

const PAUSA: Duration = Duration::from_millis(400);
const PACIENCIA: Duration = Duration::from_secs(5);
/// Lo que se espera para dar por hecho que **no** avisa: bastante más que
/// la pausa.
const SILENCIO: Duration = Duration::from_millis(1_500);

fn carpeta(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("obrain-vigia-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("01 Identidad")).unwrap();
    d
}

fn vigia(raices: &[PathBuf]) -> (Vigia, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel();
    let v = Vigia::empezar(PAUSA, move || {
        let _ = tx.send(());
    })
    .unwrap();
    v.sincronizar(raices);
    (v, rx)
}

#[test]
fn una_racha_de_archivos_es_una_sola_pasada() {
    let b = carpeta("racha");
    let (v, rx) = vigia(std::slice::from_ref(&b));
    assert_eq!(v.raices(), std::slice::from_ref(&b));

    for i in 0..20 {
        std::fs::write(b.join("01 Identidad").join(format!("doc {i}.txt")), "hola").unwrap();
    }
    rx.recv_timeout(PACIENCIA).expect("avisa tras la racha");
    assert!(
        rx.recv_timeout(SILENCIO).is_err(),
        "y sólo una vez: veinte archivos no son veinte pasadas"
    );

    // Una subcarpeta nueva también se vigila (es recursivo).
    std::fs::create_dir_all(b.join("02 Salud")).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    rx.recv_timeout(PACIENCIA)
        .expect("crear una carpeta cuenta");
    std::fs::write(b.join("02 Salud").join("receta.txt"), "x").unwrap();
    rx.recv_timeout(PACIENCIA)
        .expect("y lo que llega dentro de ella, también");
}

#[test]
fn leer_y_los_temporales_no_despiertan_a_nadie() {
    let b = carpeta("temporales");
    let doc = b.join("01 Identidad").join("Pasaporte.txt");
    std::fs::write(&doc, "pasaporte").unwrap();
    let (_v, rx) = vigia(std::slice::from_ref(&b));

    // Leer es lo que hace el propio indexado.
    let _ = std::fs::read(&doc).unwrap();
    // El candado de Word, y una descarga a medias.
    std::fs::write(b.join("01 Identidad").join("~$saporte.docx"), "x").unwrap();
    std::fs::write(b.join("01 Identidad").join("tie.jpg.crdownload"), "x").unwrap();
    assert!(
        rx.recv_timeout(SILENCIO).is_err(),
        "ni leer ni un temporal lanzan una pasada"
    );

    std::fs::write(&doc, "pasaporte renovado").unwrap();
    rx.recv_timeout(PACIENCIA)
        .expect("cambiar el documento de verdad, sí");
}

#[test]
fn lo_que_se_deja_de_vigilar_no_avisa() {
    let a = carpeta("a");
    let b = carpeta("b");
    let (v, rx) = vigia(&[a.clone(), b.clone()]);
    v.sincronizar(std::slice::from_ref(&a));
    assert_eq!(v.raices(), std::slice::from_ref(&a));

    std::fs::write(b.join("fuera.txt"), "x").unwrap();
    assert!(rx.recv_timeout(SILENCIO).is_err(), "b ya no se vigila");

    // Una biblioteca que no está en el disco se salta sin error.
    v.sincronizar(&[a.clone(), a.join("no existe")]);
    assert_eq!(v.raices(), std::slice::from_ref(&a));

    std::fs::write(a.join("dentro.txt"), "x").unwrap();
    rx.recv_timeout(PACIENCIA).expect("a sí");

    // Soltar el vigía lo para: el hilo termina y no avisa más.
    drop(v);
    std::fs::write(a.join("otra.txt"), "x").unwrap();
    assert!(rx.recv_timeout(SILENCIO).is_err());
}
