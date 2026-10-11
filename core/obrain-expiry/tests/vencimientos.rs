//! La etapa de vencimientos sobre una base de verdad: lo que escribe, lo que
//! no repite, lo que retracta y lo que **nunca** toca (ADR-0023).

use obrain_core::{EntityId, Method, Store, TimePoint};
use obrain_expiry::{ExpiryReport, Fecha, Vence, detect_all, resolved, set_by_user};

/// Un documento con su texto indexado, fragmentado como lo deja M2.
fn documento(s: &Store, texto: &str) -> EntityId {
    let e = s.create_entity(1).unwrap();
    escribir(s, &e, texto);
    e
}

/// Cambia el texto de un documento, como si se hubiera vuelto a extraer.
fn escribir(s: &Store, e: &EntityId, texto: &str) {
    obrain_extract::index::migrate(s.conn()).unwrap();
    let chunks = obrain_extract::chunk::chunk(texto);
    obrain_extract::index::replace_chunks(s.conn(), e, "paragraph-pack", 1, &chunks).unwrap();
}

fn pasada(s: &Store, at: i64) -> ExpiryReport {
    let mut r = ExpiryReport::default();
    detect_all(s, at, &mut r).unwrap();
    r
}

fn vence(s: &Store, e: &EntityId, at: i64) -> Option<(Vence, bool)> {
    resolved(s, TimePoint::at(at))
        .unwrap()
        .into_iter()
        .find(|v| &v.entity == e)
        .map(|v| (v.vence, v.by_user))
}

fn fecha(iso: &str) -> Vence {
    Vence::El(Fecha::parse_iso(iso).unwrap())
}

const TIE: &str = "TARJETA DE IDENTIDAD DE EXTRANJERO\nNIE X1234567L\nVALIDEZ 15 11 2026";

#[test]
fn la_tie_fotografiada_caduca_el_15_de_noviembre() {
    let s = Store::open_in_memory().unwrap();
    let tie = documento(&s, TIE);
    let r = pasada(&s, 10);
    assert_eq!((r.with_text, r.found, r.assertions_written), (1, 1, 1));
    assert_eq!(vence(&s, &tie, 10), Some((fecha("2026-11-15"), false)));

    let a = s
        .resolve(&tie, "expires_on", TimePoint::at(10))
        .unwrap()
        .unwrap();
    assert_eq!(a.method, Method::Inferred);
    assert_eq!(a.source, "expiry@v2");
    assert!((a.confidence - 0.85).abs() < 1e-9);
    // La evidencia es una dirección, y la dirección lleva al sitio exacto.
    let ev = a.evidence.unwrap();
    let i = TIE.find("VALIDEZ").unwrap();
    assert_eq!(
        ev,
        format!("regla=validez fragmento=0 bytes={i}..{}", TIE.len())
    );
    assert_eq!(&TIE[i..], "VALIDEZ 15 11 2026");
}

#[test]
fn la_evidencia_nunca_copia_el_documento() {
    let s = Store::open_in_memory().unwrap();
    documento(&s, TIE);
    pasada(&s, 10);
    let a = &s
        .query(None, Some("expires_on"), TimePoint::at(10))
        .unwrap()[0];
    let ev = a.evidence.as_deref().unwrap();
    for palabra in ["X1234567L", "NIE", "VALIDEZ", "TARJETA", "15 11"] {
        assert!(
            !ev.contains(palabra),
            "la evidencia copió «{palabra}»: {ev}"
        );
    }
}

#[test]
fn volver_a_pasar_no_escribe_nada() {
    let s = Store::open_in_memory().unwrap();
    documento(&s, TIE);
    documento(&s, "Una nota sin fechas.");
    pasada(&s, 10);
    let log = s.log_len().unwrap();
    let r = pasada(&s, 20);
    assert_eq!(
        (
            r.assertions_written,
            r.assertions_unchanged,
            r.assertions_retracted
        ),
        (0, 1, 0)
    );
    assert_eq!(s.log_len().unwrap(), log);
}

#[test]
fn si_el_texto_cambia_se_retracta_lo_viejo_y_se_afirma_lo_nuevo() {
    let s = Store::open_in_memory().unwrap();
    let tie = documento(&s, TIE);
    pasada(&s, 10);

    // La TIE renovada: otra fecha.
    escribir(&s, &tie, &TIE.replace("2026", "2031"));
    let r = pasada(&s, 20);
    assert_eq!((r.assertions_written, r.assertions_retracted), (1, 1));
    assert_eq!(vence(&s, &tie, 20), Some((fecha("2031-11-15"), false)));
    // I4: lo de antes no se borró. Preguntando por el pasado, sigue ahí.
    assert_eq!(vence(&s, &tie, 15), Some((fecha("2026-11-15"), false)));

    // Ya no dice ninguna fecha: la inferencia se va con el texto.
    escribir(&s, &tie, "TARJETA DE IDENTIDAD DE EXTRANJERO");
    let r = pasada(&s, 30);
    assert_eq!((r.found, r.assertions_retracted), (0, 1));
    assert_eq!(vence(&s, &tie, 30), None);
}

#[test]
fn la_palabra_del_usuario_gana_y_sobrevive_a_reindexar() {
    let s = Store::open_in_memory().unwrap();
    let tie = documento(&s, TIE);
    pasada(&s, 10);

    // El OCR leyó bien la palabra y mal el año: el usuario lo corrige.
    set_by_user(&s, &tie, Fecha::parse_iso("2027-11-15"), 20).unwrap();
    assert_eq!(vence(&s, &tie, 20), Some((fecha("2027-11-15"), true)));

    // Reindexar no lo pisa, ni aunque el texto cambie y el detector retracte
    // lo suyo.
    pasada(&s, 30);
    escribir(&s, &tie, &TIE.replace("2026", "2030"));
    pasada(&s, 40);
    assert_eq!(vence(&s, &tie, 40), Some((fecha("2027-11-15"), true)));

    // Ni aunque el texto deje de tener fecha.
    escribir(&s, &tie, "sin fechas");
    pasada(&s, 50);
    assert_eq!(vence(&s, &tie, 50), Some((fecha("2027-11-15"), true)));

    // La inferencia corregida se queda en el log: qué creía el sistema.
    let del_detector = s
        .query(
            Some(&tie),
            Some("expires_on"),
            TimePoint::as_known_at(35, 35),
        )
        .unwrap()
        .into_iter()
        .filter(|a| a.method == Method::Inferred)
        .count();
    assert_eq!(del_detector, 1);
}

#[test]
fn no_caduca_tambien_es_una_respuesta_del_usuario() {
    let s = Store::open_in_memory().unwrap();
    let doc = documento(
        &s,
        "Póliza de hogar. Periodo de cobertura: desde el 1/1/2026 hasta el 31/12/2026.",
    );
    pasada(&s, 10);
    assert_eq!(vence(&s, &doc, 10), Some((fecha("2026-12-31"), false)));
    set_by_user(&s, &doc, None, 20).unwrap();
    pasada(&s, 30);
    assert_eq!(vence(&s, &doc, 30), Some((Vence::Nunca, true)));
}

#[test]
fn con_varias_fechas_se_queda_con_una_y_cuenta_las_otras() {
    let s = Store::open_in_memory().unwrap();
    let poliza = documento(
        &s,
        "Póliza de salud Salud Mediterránea.\n\n\
         Periodo de carencia hasta el 31/12/2026 para el dental.\n\n\
         Vigencia de la póliza hasta el 31 de agosto de 2027.",
    );
    pasada(&s, 10);
    assert_eq!(vence(&s, &poliza, 10), Some((fecha("2027-08-31"), false)));
    let a = s
        .resolve(&poliza, "expires_on", TimePoint::at(10))
        .unwrap()
        .unwrap();
    assert!(a.evidence.unwrap().ends_with("otras=1"));
}

/// Cómo leyó el OCR de Windows el pasaporte de JP (2026-09-23), con los datos
/// inventados: se comió «caducidad» y «Date of expiry», y no hay MRZ.
const PASAPORTE_OCR: &str = "\
ESTADOS UNIDOS MEXICANOS
PASAPORTE
PASSPORT
Passport No
G00000000
Nombres/ Given names
JUAN PRUEBA
Fecha de Date ot bith
04 01 2004
PRUE040104HJCRRN09
a de expediciónDaXeXiX
10 06 2022
Autoridad Authority SRE-JALISCO
Fecha de
date
10 06 2032
";

#[test]
fn el_pasaporte_sin_etiquetas_caduca_diez_anios_despues_de_expedirse() {
    let s = Store::open_in_memory().unwrap();
    let pasaporte = documento(&s, PASAPORTE_OCR);
    let r = pasada(&s, 10);
    assert_eq!((r.found, r.by_rule.get("plazo")), (1, Some(&1)));
    assert_eq!(
        vence(&s, &pasaporte, 10),
        Some((fecha("2032-06-10"), false))
    );

    let a = s
        .resolve(&pasaporte, "expires_on", TimePoint::at(10))
        .unwrap()
        .unwrap();
    assert!((a.confidence - 0.7).abs() < 1e-9);
    let ev = a.evidence.unwrap();
    assert!(ev.starts_with("regla=plazo fragmento=0 bytes="), "{ev}");
    assert!(ev.contains(" anios=10"), "{ev}");
    // El tramo va de la expedición a la caducidad, y la evidencia no copia
    // ninguna de las dos.
    let (x, y) = ev["regla=plazo fragmento=0 bytes=".len()..]
        .split_whitespace()
        .next()
        .unwrap()
        .split_once("..")
        .unwrap();
    let tramo = &PASAPORTE_OCR[x.parse::<usize>().unwrap()..y.parse::<usize>().unwrap()];
    assert!(tramo.starts_with("10 06 2022") && tramo.ends_with("10 06 2032"));
    for dato in ["2022", "2032", "G00000000", "PRUEBA"] {
        assert!(!ev.contains(dato), "la evidencia copió «{dato}»: {ev}");
    }
}

#[test]
fn lo_que_escribio_la_version_anterior_sigue_siendo_suyo() {
    use obrain_core::{NewAssertion, Value};
    let s = Store::open_in_memory().unwrap();
    let tie = documento(&s, TIE);
    let otra = documento(&s, "Contrato. Vence el 30/09/2026.");
    let i = TIE.find("VALIDEZ").unwrap();
    // Lo que la v1 dejó escrito: lo mismo que diría hoy, y algo que ya no.
    for (e, valor, ev) in [
        (
            &tie,
            "2026-11-15",
            format!("regla=validez fragmento=0 bytes={i}..{}", TIE.len()),
        ),
        (
            &otra,
            "2026-10-01",
            "regla=vencimiento fragmento=0 bytes=0..5".to_string(),
        ),
    ] {
        s.assert(
            NewAssertion::new(
                e.clone(),
                "expires_on",
                Value::Text(valor.into()),
                Method::Inferred,
                5,
            )
            .source("expiry@v1")
            .confidence(0.8)
            .evidence(&ev),
        )
        .unwrap();
    }
    let r = pasada(&s, 10);
    // La TIE no se reescribe; el contrato se corrige, retractando lo de la v1.
    assert_eq!(
        (
            r.assertions_unchanged,
            r.assertions_written,
            r.assertions_retracted
        ),
        (1, 1, 1)
    );
    assert_eq!(vence(&s, &otra, 10), Some((fecha("2026-09-30"), false)));
}
