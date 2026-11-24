//! Contratos y pólizas se juzgan por lo que traen escrito ([ADR-0040]).
//!
//! «Contrato» o «póliza» en el nombre, o en la carpeta, protege lo que Brain
//! no ha podido leer. Cuando el texto de la versión actual está leído, decide
//! el contenido: un NIE, un IBAN o una tarjeta lo dejan sensible; nada de eso,
//! normal. Todo es inventado: ni rutas ni documentos de nadie.
//!
//! [ADR-0040]: ../../../docs/adr/0040-un-contrato-se-juzga-por-lo-que-trae.md

use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint};
use obrain_guard::{GuardReport, classify_all};

const T: i64 = 1_000_000;

const COBERTURAS: &str = "Coberturas: incendio, daños por agua y robo. Franquicia de 150 EUR.";
const CON_NIE: &str = "Tomador: persona inventada, NIE X1234567L. Prima anual 312 EUR.";

fn base() -> Store {
    let s = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(s.conn()).unwrap();
    s
}

/// El hash que M1 observó de la versión actual de cada archivo.
const OBSERVADO: &str = "hash-de-la-version-observada";

fn archivo(s: &Store, ruta: &str) -> EntityId {
    let e = s.create_entity(T).unwrap();
    for (p, v) in [
        ("path_observed_at", ruta),
        ("availability", "local"),
        ("has_hash", OBSERVADO),
    ] {
        s.assert(NewAssertion::new(e.clone(), p, v, Method::Deterministic, T))
            .unwrap();
    }
    e
}

/// Lo que M2 deja al leer un archivo: su extracción, con el hash del archivo
/// que leyó, y el texto en `chunks`.
fn extraido(s: &Store, e: &EntityId, hash: &str, status: &str, texto: &str) {
    use obrain_extract::chunk::{CHUNKER, CHUNKER_VERSION};
    obrain_extract::index::record(
        s.conn(),
        obrain_extract::index::Record {
            entity: e,
            extractor: "txt",
            extractor_ver: 1,
            chunker: CHUNKER,
            chunker_ver: CHUNKER_VERSION,
            status,
            file_hash: Some(hash),
            text_hash: None,
            chars: texto.len(),
            chunk_count: 1,
            at: T,
            detail: None,
        },
    )
    .unwrap();
    s.conn()
        .execute(
            "INSERT INTO chunks (entity, chunker, chunker_ver, ordinal, byte_start, byte_end, text)
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?5)",
            obrain_core::rusqlite::params![
                e.as_str(),
                CHUNKER,
                CHUNKER_VERSION,
                texto.len() as i64,
                texto
            ],
        )
        .unwrap();
}

/// La versión actual, leída entera.
fn leido(s: &Store, e: &EntityId, texto: &str) {
    extraido(s, e, OBSERVADO, "ok", texto);
}

fn clasificar(s: &Store, at: i64) -> GuardReport {
    let filas = obrain_extract::load_files(s, TimePoint::at(at)).unwrap();
    let mut r = GuardReport::default();
    classify_all(s, &filas, at, false, &mut r).unwrap();
    assert!(r.arithmetic_holds());
    r
}

fn nivel(s: &Store, e: &EntityId) -> Sensitivity {
    s.sensitivity(e).unwrap()
}

/// El caso de T3: la póliza de hogar leída, con coberturas y sin datos de
/// nadie, ya no se esconde por su nombre.
#[test]
fn una_poliza_leida_sin_datos_personales_es_normal() {
    let s = base();
    let poliza = archivo(&s, r"C:\Biblioteca\poliza-hogar-aurora-2026.txt");
    let contrato = archivo(&s, r"C:\Biblioteca\Contrato de alquiler.pdf");
    leido(&s, &poliza, COBERTURAS);
    leido(
        &s,
        &contrato,
        "Renta: 780 EUR al mes. Fianza: una mensualidad.",
    );

    let r = clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &poliza), Sensitivity::Normal);
    assert_eq!(nivel(&s, &contrato), Sensitivity::Normal);
    assert_eq!(r.by_rule.get("nombre:contractual"), None);
}

/// Lo que de verdad hace sensible a un contrato sigue dentro: el NIE.
#[test]
fn una_poliza_con_un_nie_sigue_siendo_sensible() {
    let s = base();
    let poliza = archivo(&s, r"C:\Biblioteca\Póliza hogar.pdf");
    leido(&s, &poliza, CON_NIE);

    let r = clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &poliza), Sensitivity::Sensitive);
    assert_eq!(r.by_rule.get("contenido:dni-o-nie"), Some(&1));
}

/// Sin texto —un escaneo, un archivo deshidratado— el contenido no puede
/// contradecir al nombre, así que el nombre protege, como antes.
#[test]
fn un_contrato_sin_texto_se_protege_por_su_nombre() {
    let s = base();
    let escaneo = archivo(&s, r"C:\Biblioteca\Contrato firmado.jpg");

    let r = clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &escaneo), Sensitivity::Sensitive);
    assert_eq!(r.by_rule.get("nombre:contractual"), Some(&1));
}

/// El texto de una versión anterior no es el de ésta: la nueva puede traer el
/// NIE que la vieja no traía. Tampoco cuenta una extracción que falló.
#[test]
fn solo_cuenta_el_texto_de_la_version_observada() {
    let s = base();
    let anterior = archivo(&s, r"C:\Biblioteca\contrato-gimnasio.txt");
    let fallida = archivo(&s, r"C:\Biblioteca\poliza-moto.txt");
    extraido(
        &s,
        &anterior,
        "hash-de-la-version-anterior",
        "ok",
        COBERTURAS,
    );
    extraido(&s, &fallida, OBSERVADO, "failed", COBERTURAS);

    clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &anterior), Sensitivity::Sensitive);
    assert_eq!(nivel(&s, &fallida), Sensitivity::Sensitive);
}

/// La carpeta `Contratos` cede igual que el nombre; `Contratos y DNI`, no.
#[test]
fn la_carpeta_de_contratos_cede_ante_el_texto_leido() {
    let s = base();
    let leida = archivo(&s, r"C:\Biblioteca\Contratos\condiciones-generales.txt");
    let sin_texto = archivo(&s, r"C:\Biblioteca\Contratos\IMG_0042.jpg");
    let con_dni = archivo(&s, r"C:\Biblioteca\Contratos y DNI\condiciones.txt");
    leido(&s, &leida, COBERTURAS);
    leido(&s, &con_dni, COBERTURAS);

    clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &leida), Sensitivity::Normal);
    assert_eq!(nivel(&s, &sin_texto), Sensitivity::Sensitive);
    assert_eq!(nivel(&s, &con_dni), Sensitivity::Sensitive);
}

/// Al leerse el texto, la señal del nombre se retracta (queda en el log con su
/// fecha de baja) y la siguiente pasada no escribe nada.
#[test]
fn leer_el_texto_retracta_la_senal_del_nombre() {
    let s = base();
    let poliza = archivo(&s, r"C:\Biblioteca\poliza-coche.pdf");

    clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &poliza), Sensitivity::Sensitive);

    leido(&s, &poliza, COBERTURAS);
    let r = clasificar(&s, T + 2);
    assert_eq!(nivel(&s, &poliza), Sensitivity::Normal);
    assert_eq!(r.assertions_retracted, 1);

    let r = clasificar(&s, T + 3);
    assert_eq!(r.assertions_written + r.assertions_retracted, 0);
}

/// Sólo contrato y póliza ceden: una nómina leída sin IBAN sigue sensible,
/// porque el sueldo no tiene forma que el contenido reconozca.
#[test]
fn las_demas_senales_de_nombre_no_ceden() {
    let s = base();
    let nomina = archivo(&s, r"C:\Biblioteca\Nómina marzo.pdf");
    let clave = archivo(&s, r"C:\Biblioteca\contraseña del router.txt");
    leido(&s, &nomina, "Salario base 1.000 EUR.");
    leido(&s, &clave, "wifi de casa");

    clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &nomina), Sensitivity::Sensitive);
    assert_eq!(nivel(&s, &clave), Sensitivity::Vault);
}
