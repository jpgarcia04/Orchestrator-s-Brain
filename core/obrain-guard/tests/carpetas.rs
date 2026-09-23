//! Sensibilidad por carpeta ([ADR-0020]) contra el clasificador de verdad.
//!
//! Las reglas de resolución se prueban en `obrain-core`; aquí se prueba lo que
//! el usuario va a hacer con ellas: marcar `01 Identidad`, que la foto de la TIE
//! salga sensible, que lo que llegue después también, y que ningún reescaneo lo
//! deshaga.
//!
//! [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md

use obrain_core::{
    CONTENTS_SENSITIVITY, EntityId, Error, Method, NewAssertion, Sensitivity, Store, TimePoint,
};
use obrain_guard::{GuardReport, classify_all, mark_by_user, mark_folder};

const T: i64 = 1_000_000;

/// Una base con las tablas de M2: el clasificador lee `chunks`.
fn base() -> Store {
    let s = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(s.conn()).unwrap();
    s
}

fn carpeta(s: &Store, ruta: &str) -> EntityId {
    let e = s.create_entity(T).unwrap();
    s.assert(NewAssertion::new(
        e.clone(),
        "folder_path",
        ruta,
        Method::Deterministic,
        T,
    ))
    .unwrap();
    e
}

/// Un archivo como lo deja M1: ruta y disponibilidad.
fn archivo(s: &Store, ruta: &str, at: i64) -> EntityId {
    let e = s.create_entity(at).unwrap();
    for (p, v) in [("path_observed_at", ruta), ("availability", "local")] {
        s.assert(NewAssertion::new(
            e.clone(),
            p,
            v,
            Method::Deterministic,
            at,
        ))
        .unwrap();
    }
    e
}

/// Una pasada del clasificador, como la hace `obrain indexar`.
fn clasificar(s: &Store, at: i64) -> GuardReport {
    let filas = obrain_extract::load_files(s, TimePoint::at(at)).unwrap();
    let mut r = GuardReport::default();
    classify_all(s, &filas, at, false, &mut r).unwrap();
    r
}

fn nivel(s: &Store, e: &EntityId) -> Sensitivity {
    s.sensitivity(e).unwrap()
}

/// El caso que abrió la Fase 3, de punta a punta.
#[test]
fn marcar_identidad_protege_la_foto_de_la_tie() {
    let s = base();
    let boveda = carpeta(&s, r"C:\Bóveda");
    let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
    let tie = archivo(&s, r"C:\Bóveda\01 Identidad\IMG_2291.jpg", T);
    let salud = archivo(&s, r"C:\Bóveda\02 Salud\receta.jpg", T);

    // Sin marca, la foto no tiene ni nombre útil ni texto: sale normal.
    clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &tie), Sensitivity::Normal);

    let r = mark_folder(&s, &identidad, Some(Sensitivity::Sensitive), T + 2).unwrap();
    assert_eq!(
        nivel(&s, &tie),
        Sensitivity::Sensitive,
        "sube al marcar, sin reindexar"
    );
    assert_eq!(nivel(&s, &identidad), Sensitivity::Sensitive);
    assert_eq!(nivel(&s, &salud), Sensitivity::Normal);
    assert_eq!(
        nivel(&s, &boveda),
        Sensitivity::Normal,
        "la madre no hereda de la hija"
    );
    assert_eq!(r.under, 2, "la carpeta y la foto");
    assert_eq!(r.changed, 2);
}

/// «También lo que llegue después»: un archivo nuevo dentro de una carpeta
/// marcada sale sensible en el primer indexado que lo ve.
#[test]
fn lo_que_llega_despues_tambien_hereda() {
    let s = base();
    let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
    mark_folder(&s, &identidad, Some(Sensitivity::Sensitive), T).unwrap();

    let nuevo = archivo(&s, r"C:\Bóveda\01 Identidad\2027\IMG_0042.jpg", T + 10);
    let r = clasificar(&s, T + 11);
    assert_eq!(nivel(&s, &nuevo), Sensitivity::Sensitive);
    assert_eq!(r.inherited, 1);
    assert_eq!(r.effective, [0, 1, 0]);
    // El clasificador no dijo nada de él: lo que cuenta como normal es su
    // veredicto, no lo que lee la puerta.
    assert_eq!(r.normal, 1);
    assert!(r.arithmetic_holds());
}

/// Una subcarpeta que aparece después también hereda: enseña su candado en el
/// explorador igual que sus archivos, aunque las carpetas no pasen por las
/// reglas.
#[test]
fn una_subcarpeta_nueva_tambien_hereda() {
    let s = base();
    let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
    mark_folder(&s, &identidad, Some(Sensitivity::Sensitive), T).unwrap();

    let nueva = carpeta(&s, r"C:\Bóveda\01 Identidad\Renovación 2027");
    let otra = carpeta(&s, r"C:\Bóveda\02 Salud");
    assert_eq!(
        nivel(&s, &nueva),
        Sensitivity::Normal,
        "todavía nadie la ha mirado"
    );
    clasificar(&s, T + 1);
    assert_eq!(nivel(&s, &nueva), Sensitivity::Sensitive);
    assert_eq!(nivel(&s, &otra), Sensitivity::Normal);
}

/// I5: ningún reescaneo pisa la marca. Se reclasifica varias veces y la marca
/// sigue igual, sin reescribirse ni retractarse.
#[test]
fn ningun_reescaneo_pisa_la_marca() {
    let s = base();
    let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
    let tie = archivo(&s, r"C:\Bóveda\01 Identidad\IMG_2291.jpg", T);
    mark_folder(&s, &identidad, Some(Sensitivity::Sensitive), T + 1).unwrap();

    let antes = s.log_len().unwrap();
    for i in 0..3 {
        clasificar(&s, T + 10 + i);
    }
    assert_eq!(nivel(&s, &tie), Sensitivity::Sensitive);
    let marcas = s
        .query(
            Some(&identidad),
            Some(CONTENTS_SENSITIVITY),
            TimePoint::at(T + 20),
        )
        .unwrap();
    assert_eq!(marcas.len(), 1, "la marca sigue viva y es una sola");
    assert_eq!(s.log_len().unwrap(), antes, "reclasificar no escribe nada");
}

/// Quitar la marca devuelve cada archivo a lo que diga el clasificador —que
/// puede seguir diciendo sensible por el nombre— y deja el rastro en el log.
#[test]
fn quitar_la_marca_devuelve_lo_que_diga_el_clasificador() {
    let s = base();
    let identidad = carpeta(&s, r"C:\Bóveda\01 Identidad");
    let tie = archivo(&s, r"C:\Bóveda\01 Identidad\IMG_2291.jpg", T);
    let pasaporte = archivo(&s, r"C:\Bóveda\01 Identidad\Pasaporte.pdf", T);
    clasificar(&s, T + 1);
    mark_folder(&s, &identidad, Some(Sensitivity::Sensitive), T + 2).unwrap();

    let r = mark_folder(&s, &identidad, None, T + 3).unwrap();
    assert_eq!(r.level, None);
    assert_eq!(nivel(&s, &tie), Sensitivity::Normal);
    assert_eq!(
        nivel(&s, &pasaporte),
        Sensitivity::Sensitive,
        "su nombre lo sigue delatando"
    );

    // La marca no se borró: se retractó (I4). En T+2 existía.
    assert!(
        s.folder_marks(TimePoint::at(T + 2))
            .unwrap()
            .of(&identidad)
            .is_some()
    );
    assert!(s.folder_marks(TimePoint::at(T + 4)).unwrap().is_empty());
}

/// Marcar dos veces lo mismo no escribe dos veces (ADR-0001). Cambiar de nivel
/// retracta la anterior: la cardinalidad es `one`.
#[test]
fn marcar_es_idempotente_y_cambiar_de_nivel_sustituye() {
    let s = base();
    let c = carpeta(&s, r"C:\Bóveda\Claves");
    let f = archivo(&s, r"C:\Bóveda\Claves\router.txt", T);

    mark_folder(&s, &c, Some(Sensitivity::Sensitive), T + 1).unwrap();
    let n = s.log_len().unwrap();
    mark_folder(&s, &c, Some(Sensitivity::Sensitive), T + 2).unwrap();
    assert_eq!(s.log_len().unwrap(), n);

    mark_folder(&s, &c, Some(Sensitivity::Vault), T + 3).unwrap();
    assert_eq!(nivel(&s, &f), Sensitivity::Vault);
    let vivas = s
        .query(Some(&c), Some(CONTENTS_SENSITIVITY), TimePoint::at(T + 4))
        .unwrap();
    assert_eq!(vivas.len(), 1);
    assert_eq!(vivas[0].object.as_text(), Some("vault"));
    assert!(s.contradictions(TimePoint::at(T + 4)).unwrap().is_empty());
}

/// La excepción consciente: un archivo que el usuario marca como normal
/// **después** de marcar la carpeta, se queda normal aunque se reclasifique.
#[test]
fn una_excepcion_posterior_se_respeta() {
    let s = base();
    let c = carpeta(&s, r"C:\Bóveda\04 Trámites");
    let folleto = archivo(&s, r"C:\Bóveda\04 Trámites\folleto del gimnasio.pdf", T);
    mark_folder(&s, &c, Some(Sensitivity::Sensitive), T + 1).unwrap();
    assert_eq!(nivel(&s, &folleto), Sensitivity::Sensitive);

    mark_by_user(&s, &folleto, Sensitivity::Normal, T + 2).unwrap();
    clasificar(&s, T + 3);
    assert_eq!(nivel(&s, &folleto), Sensitivity::Normal);
}

#[test]
fn solo_se_marcan_carpetas_y_solo_hacia_arriba() {
    let s = base();
    let c = carpeta(&s, r"C:\Bóveda\01 Identidad");
    let f = archivo(&s, r"C:\Bóveda\01 Identidad\x.pdf", T);

    assert!(matches!(
        mark_folder(&s, &f, Some(Sensitivity::Sensitive), T + 1),
        Err(Error::NotAFolder(_))
    ));
    assert!(matches!(
        mark_folder(&s, &c, Some(Sensitivity::Normal), T + 1),
        Err(Error::InvalidMark(_))
    ));
}
