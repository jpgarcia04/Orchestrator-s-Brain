//! Cifrado en reposo y PIN ([ADR-0021]) desde la superficie: lo que hace la app
//! al abrir la base, y lo que el PIN deja salir del núcleo.
//!
//! [ADR-0021]: ../../../docs/adr/0021-cifrado-en-reposo-y-pin.md

use obrain_api::seguridad::{self, abrir_con};
use obrain_api::{Brain, Error};
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store};
use obrain_extract::chunk::Chunk;
use obrain_keys::KdfParams;
use std::path::{Path, PathBuf};

const K: KdfParams = KdfParams::FAST_FOR_TESTS;

fn dir(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("obrain-seg-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn en_claro(p: &Path) -> bool {
    std::fs::read(p).map(|b| b.windows(9).any(|w| w == b"PASAPORTE")).unwrap_or(false)
}

fn con_sufijo(p: &Path, s: &str) -> PathBuf {
    let mut o = p.as_os_str().to_owned();
    o.push(s);
    PathBuf::from(o)
}

/// Un documento con texto indexado, como lo deja M2.
fn documento(s: &Store, ruta: &str, texto: &str) -> EntityId {
    let e = s.create_entity(1).unwrap();
    s.assert(NewAssertion::new(e.clone(), "path_observed_at", ruta, Method::Deterministic, 1)).unwrap();
    obrain_extract::index::migrate(s.conn()).unwrap();
    obrain_extract::index::replace_chunks(
        s.conn(),
        &e,
        "paragraph-pack",
        1,
        &[Chunk { ordinal: 0, byte_start: 0, byte_end: texto.len(), text: texto.to_string() }],
    )
    .unwrap();
    e
}

// ═══════════════════════════════════════════════════════════════ el cifrado

/// Primera vez: no hay base. Se crea **cifrada** y la clave de recuperación
/// queda pendiente de enseñar.
#[test]
fn una_base_nueva_nace_cifrada() {
    let d = dir("nueva");
    let db = d.join("brain.obrain");
    let b = Brain::from_store(abrir_con(&db, K).unwrap());

    let e = b.security().unwrap();
    assert!(e.cifrada);
    assert_eq!(e.proteccion.as_deref(), Some(if cfg!(windows) { "dpapi" } else { "sin-proteger" }));
    assert!(e.recuperacion_pendiente);
    assert!(!e.pin);

    let codigo = b.recovery_code().unwrap().expect("tiene que haber clave de recuperación");
    assert_eq!(codigo.len(), 29);
    b.confirm_recovery_saved().unwrap();
    assert!(!b.security().unwrap().recuperacion_pendiente);
}

/// La base de agosto, en claro, se cifra sola al abrirla: todo sigue ahí, en el
/// disco no queda nada en claro y no quedan restos del cambio.
#[test]
fn una_base_en_claro_se_cifra_al_abrirla() {
    let d = dir("migrar");
    let db = d.join("brain.obrain");
    let doc = {
        let s = Store::open(&db).unwrap();
        documento(&s, r"C:\Bóveda\01 Identidad\pasaporte.pdf", "PASAPORTE G00000000 valido hasta 2031")
    };
    assert!(en_claro(&db));

    let b = Brain::from_store(abrir_con(&db, K).unwrap());
    assert!(b.security().unwrap().cifrada);
    assert!(b.security().unwrap().recuperacion_pendiente);
    assert!(b.preview(&doc, None).unwrap().text.unwrap().contains("G00000000"));
    drop(b);

    assert!(!en_claro(&db), "después de cifrar queda texto en claro");
    for resto in [".cifrando", ".cifrando-wal", ".llave.tmp"] {
        assert!(!con_sufijo(&db, resto).exists(), "queda {resto}");
    }
    // Y abrirla otra vez no vuelve a cifrar nada: la clave sigue siendo la misma.
    let codigo = Brain::from_store(abrir_con(&db, K).unwrap()).recovery_code().unwrap();
    let otra = Brain::from_store(abrir_con(&db, K).unwrap()).recovery_code().unwrap();
    assert_eq!(codigo, otra);
}

/// Un corte de luz entre escribir la llave y sustituir la base: la próxima
/// apertura termina el cambio.
#[test]
fn un_cifrado_a_medias_se_termina_al_abrir() {
    let d = dir("medias");
    let db = d.join("brain.obrain");
    let doc = {
        let s = Store::open(&db).unwrap();
        seguridad::migrate(s.conn()).unwrap();
        documento(&s, r"C:\x\pasaporte.pdf", "PASAPORTE G00000000")
    };
    // Los pasos 1 a 3 de `cifrar`, y el corte.
    let pendiente = obrain_keys::prepare(K).unwrap();
    Store::export_encrypted(&db, con_sufijo(&db, ".cifrando"), pendiente.key()).unwrap();
    pendiente.write(&db).unwrap();
    assert!(en_claro(&db), "el corte deja la original en claro");

    let b = Brain::from_store(abrir_con(&db, K).unwrap());
    assert!(b.preview(&doc, None).unwrap().text.unwrap().contains("G00000000"));
    drop(b);
    assert!(!en_claro(&db));
    assert!(!con_sufijo(&db, ".cifrando").exists());
}

/// Otro equipo (o Windows reinstalado): la llave no se abre. El error lo dice,
/// y con la clave de recuperación vuelve a abrir sola.
#[test]
fn sin_la_llave_de_windows_se_entra_con_la_clave_de_recuperacion() {
    let d = dir("recuperar");
    let db = d.join("brain.obrain");
    let codigo = Brain::from_store(abrir_con(&db, K).unwrap()).recovery_code().unwrap().unwrap();

    let llave = obrain_keys::key_path(&db);
    let propio = if cfg!(windows) { "dpapi" } else { "sin-proteger" };
    let ajeno = if cfg!(windows) { "sin-proteger" } else { "dpapi" };
    let texto = std::fs::read_to_string(&llave).unwrap().replace(propio, ajeno);
    std::fs::write(&llave, texto).unwrap();

    let err = Brain::open(&db).err().expect("no debería abrir");
    assert!(err.is_key_unavailable(), "el error tiene que decir que falta la llave: {err}");

    assert!(seguridad::recuperar(&db, "AAAAA-AAAAA-AAAAA-AAAAA-AAAAA").is_err());
    seguridad::recuperar(&db, &codigo.to_lowercase()).unwrap();
    assert!(Brain::open(&db).is_ok());
}

// ═══════════════════════════════════════════════════════════════════ el PIN

fn cerebro_con_pasaporte() -> (Brain, EntityId, EntityId, PathBuf) {
    let d = dir(&format!("pin-{}", obrain_core::EntityId::new()));
    let db = d.join("brain.obrain");
    let store = abrir_con(&db, K).unwrap();
    let pasaporte = documento(&store, r"C:\Bóveda\01 Identidad\pasaporte.pdf", "PASAPORTE numero G00000000");
    let receta = documento(&store, r"C:\Bóveda\02 Salud\receta.txt", "receta de ibuprofeno numero 600");
    obrain_guard::mark_by_user(&store, &pasaporte, Sensitivity::Sensitive, 2).unwrap();
    (Brain::from_store(store), pasaporte, receta, db)
}

/// Sin PIN, todo como antes: el contenido sale y la ventana lo tapa.
#[test]
fn sin_pin_lo_sensible_sale_como_siempre() {
    let (b, pasaporte, _, _) = cerebro_con_pasaporte();
    let p = b.preview(&pasaporte, None).unwrap();
    assert!(!p.locked);
    assert!(p.text.is_some());
    assert!(b.may_open(&pasaporte).unwrap());
}

/// Con PIN y sin desbloquear, **el contenido no sale del núcleo**: ni la vista
/// previa, ni el fragmento en la búsqueda, ni abrirlo. Lo normal, igual que
/// siempre.
#[test]
fn con_pin_lo_sensible_no_sale_hasta_desbloquear() {
    let (b, pasaporte, receta, _) = cerebro_con_pasaporte();
    b.set_pin("2468", None).unwrap();
    b.lock();

    let p = b.preview(&pasaporte, None).unwrap();
    assert!(p.locked);
    assert!(p.text.is_none() && p.image.is_none());
    assert!(!b.may_open(&pasaporte).unwrap());
    assert!(matches!(b.recovery_code(), Err(Error::Bloqueado)));

    let r = b.search("numero", 10).unwrap();
    let del_pasaporte = r.hits.iter().find(|h| h.entity == pasaporte.0).expect("sale en la lista");
    assert!(del_pasaporte.snippet.is_empty(), "el fragmento de lo sensible no puede salir");
    let de_la_receta = r.hits.iter().find(|h| h.entity == receta.0).unwrap();
    assert!(!de_la_receta.snippet.is_empty(), "lo normal sigue con su fragmento");
    assert!(b.preview(&receta, None).unwrap().text.is_some());

    // Un PIN equivocado no desbloquea.
    assert!(b.unlock("1111").is_err());
    assert!(b.preview(&pasaporte, None).unwrap().locked);

    b.unlock("2468").unwrap();
    assert!(b.security().unwrap().desbloqueada);
    assert!(b.preview(&pasaporte, None).unwrap().text.is_some());
    assert!(b.may_open(&pasaporte).unwrap());
    assert!(b.recovery_code().unwrap().is_some());
}

/// Cambiar o quitar el PIN exige saber el actual.
#[test]
fn cambiar_o_quitar_el_pin_pide_el_actual() {
    let (b, _, _, _) = cerebro_con_pasaporte();
    b.set_pin("2468", None).unwrap();
    assert!(b.set_pin("1357", None).is_err());
    assert!(b.set_pin("1357", Some("0000")).is_err());
    b.set_pin("1357", Some("2468")).unwrap();
    assert!(b.remove_pin("2468").is_err());
    b.remove_pin("1357").unwrap();
    assert!(!b.security().unwrap().pin);
}

/// Olvidé el PIN: con la clave de recuperación se pone uno nuevo. Con otra
/// clave, no.
#[test]
fn un_pin_olvidado_se_restablece_con_la_clave_de_recuperacion() {
    let (b, pasaporte, _, _) = cerebro_con_pasaporte();
    b.set_pin("2468", None).unwrap();
    let codigo = b.recovery_code().unwrap().unwrap();
    b.lock();

    assert!(b.reset_pin("AAAAA-AAAAA-AAAAA-AAAAA-AAAAA", Some("9999")).is_err());
    b.reset_pin(&codigo, Some("9999")).unwrap();
    b.lock();
    assert!(b.unlock("2468").is_err());
    b.unlock("9999").unwrap();
    assert!(b.preview(&pasaporte, None).unwrap().text.is_some());
}

/// La espera tras los fallos se guarda en la base: cerrar y abrir la app no la
/// salta. Se prueba con el reloj en la mano.
#[test]
fn la_espera_tras_los_fallos_sobrevive_a_cerrar_la_app() {
    let (b, _, _, db) = cerebro_con_pasaporte();
    b.set_pin("2468", None).unwrap();
    let t = 1_000_000;
    let conn = b.store().conn();
    for _ in 0..3 {
        assert!(matches!(seguridad::comprobar_pin(conn, "0000", t), Err(seguridad::Error::PinIncorrecto)));
    }
    // Ni siquiera el bueno entra mientras dura la espera.
    assert!(matches!(seguridad::comprobar_pin(conn, "2468", t + 1_000), Err(seguridad::Error::PinEnEspera(_))));
    drop(b);

    let b = Brain::from_store(abrir_con(&db, K).unwrap());
    let conn = b.store().conn();
    assert!(matches!(seguridad::comprobar_pin(conn, "2468", t + 1_000), Err(seguridad::Error::PinEnEspera(_))));
    seguridad::comprobar_pin(conn, "2468", t + 31_000).unwrap();
}
