//! Cifrado en reposo ([ADR-0021]): lo que se escribe en el disco no se lee sin
//! la llave, y cifrar una base en claro no pierde nada de lo que la hace ser
//! la verdad (I1, I4).
//!
//! [ADR-0021]: ../../../docs/adr/0021-cifrado-en-reposo-y-pin.md

use obrain_core::{Error, Method, NewAssertion, Store, TimePoint};
use obrain_keys::KdfParams;
use std::path::{Path, PathBuf};

const SECRETO: &str = "PASAPORTE G00000000 EJEMPLO MUESTRA";

fn dir(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("obrain-cifrado-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// ¿Aparece el secreto en algún archivo de la base (principal, WAL o SHM)?
fn en_claro(db: &Path) -> bool {
    ["", "-wal", "-shm"].iter().any(|suf| {
        let mut p = db.as_os_str().to_owned();
        p.push(suf);
        std::fs::read(PathBuf::from(p))
            .map(|b| b.windows(9).any(|w| w == b"PASAPORTE"))
            .unwrap_or(false)
    })
}

fn escribir(s: &Store) {
    let e = s.create_entity(1).unwrap();
    s.assert(NewAssertion::new(e, "about", SECRETO, Method::User, 1))
        .unwrap();
}

fn leer(s: &Store) -> Vec<String> {
    s.query(None, Some("about"), TimePoint::at(2))
        .unwrap()
        .into_iter()
        .filter_map(|a| a.object.as_text().map(str::to_string))
        .collect()
}

#[test]
fn con_llave_no_hay_nada_en_claro_en_el_disco() {
    let d = dir("disco");
    let db = d.join("brain.obrain");
    let c = obrain_keys::create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();
    {
        let s = Store::create_with(&db, Some(&c.key)).unwrap();
        escribir(&s);
        assert_eq!(leer(&s), vec![SECRETO]);
    }
    assert!(!en_claro(&db), "el texto aparece en claro en el disco");

    // `Store::open` ve la llave y abre sola.
    let s = Store::open(&db).unwrap();
    assert_eq!(leer(&s), vec![SECRETO]);
}

#[test]
fn sin_llave_o_con_otra_no_se_abre() {
    let d = dir("otra");
    let db = d.join("brain.obrain");
    let c = obrain_keys::create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();
    escribir(&Store::create_with(&db, Some(&c.key)).unwrap());

    // En claro, como si no tuviera llave.
    assert!(Store::open_with(&db, None).is_err());

    // Con la llave de otra base.
    let otra = obrain_keys::create_with(&d.join("otra.obrain"), KdfParams::FAST_FOR_TESTS).unwrap();
    assert!(matches!(
        Store::open_with(&db, Some(&otra.key)),
        Err(Error::WrongKey)
    ));
}

/// La migración de una base en claro: todo llega —también los triggers que
/// imponen I1 e I4 y una tabla FTS5 como la de M2— y en el archivo nuevo no
/// queda nada en claro.
#[test]
fn cifrar_una_base_en_claro_no_pierde_nada() {
    let d = dir("migrar");
    let claro = d.join("claro.obrain");
    let cifrada = d.join("brain.obrain");
    {
        let s = Store::open(&claro).unwrap();
        escribir(&s);
        s.conn()
            .execute_batch(
                "CREATE VIRTUAL TABLE fts USING fts5(texto);
                 INSERT INTO fts(texto) VALUES ('poliza de seguro medico con vigencia');",
            )
            .unwrap();
    }
    assert!(en_claro(&claro), "la base de partida sí está en claro");

    let c = obrain_keys::create_with(&cifrada, KdfParams::FAST_FOR_TESTS).unwrap();
    Store::export_encrypted(&claro, &cifrada, &c.key).unwrap();
    assert!(!en_claro(&cifrada));

    let s = Store::open(&cifrada).unwrap();
    assert_eq!(leer(&s), vec![SECRETO]);
    let n: i64 = s
        .conn()
        .query_row(
            "SELECT count(*) FROM fts WHERE fts MATCH 'vigencia'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "el índice FTS5 llega y sigue buscando");

    // I4 sigue impuesto por el motor.
    let borrar = s.conn().execute("DELETE FROM assertions", []);
    assert!(borrar.is_err(), "los triggers de I4 se perdieron al cifrar");
}

#[test]
fn exportar_instantanea_de_base_ya_cifrada_con_wal_activo_no_pierde_nada() {
    let d = dir("instantanea-wal");
    let db = d.join("brain.obrain");
    let snap = d.join("instantanea.obrain");
    let c = obrain_keys::create_with(&db, KdfParams::FAST_FOR_TESTS).unwrap();

    let writer = Store::create_with(&db, Some(&c.key)).unwrap();
    let reader = Store::open_with(&db, Some(&c.key)).unwrap();
    escribir(&writer);
    writer
        .conn()
        .execute_batch(
            "CREATE VIRTUAL TABLE fts USING fts5(texto);
             INSERT INTO fts(texto) VALUES ('contrato sintetico en el wal');",
        )
        .unwrap();

    // Mantener una lectura abierta en `reader` mientras `writer` añade otra
    // transacción al WAL: así las páginas nuevas viven en `brain.obrain-wal`
    // y un `wal_checkpoint(TRUNCATE)` no podría truncar el WAL.
    reader.conn().execute_batch("BEGIN DEFERRED").unwrap();
    let _: i64 = reader
        .conn()
        .query_row("SELECT count(*) FROM entities", [], |r| r.get(0))
        .unwrap();

    let e2 = writer.create_entity(10).unwrap();
    writer
        .assert(NewAssertion::new(
            e2,
            "about",
            "TRANSACCION EN WAL SIN CHECKPOINT",
            Method::User,
            10,
        ))
        .unwrap();

    Store::export_snapshot_encrypted(&db, &snap, &c.key).unwrap();
    reader.conn().execute_batch("COMMIT").unwrap();

    assert!(snap.is_file());
    assert!(!PathBuf::from(format!("{}-wal", snap.display())).exists());
    assert!(!PathBuf::from(format!("{}-shm", snap.display())).exists());
    assert!(!en_claro(&snap));

    Store::verify_snapshot(&snap, &c.key).unwrap();

    let otra = obrain_keys::create_with(&d.join("otra.obrain"), KdfParams::FAST_FOR_TESTS).unwrap();
    assert!(matches!(
        Store::verify_snapshot(&snap, &otra.key),
        Err(Error::WrongKey)
    ));

    let s = Store::open_with(&snap, Some(&c.key)).unwrap();
    let textos = s
        .query(None, Some("about"), TimePoint::at(20))
        .unwrap()
        .into_iter()
        .filter_map(|a| a.object.as_text().map(str::to_string))
        .collect::<Vec<_>>();
    assert_eq!(textos.len(), 2);
    assert!(textos.contains(&SECRETO.to_string()));
    assert!(textos.contains(&"TRANSACCION EN WAL SIN CHECKPOINT".to_string()));
}
