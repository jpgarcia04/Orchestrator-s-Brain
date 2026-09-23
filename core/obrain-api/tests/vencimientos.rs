//! Vencimientos de punta a punta ([ADR-0023]): `indexar` una bóveda, la
//! tarjeta «Caduca pronto», corregir una fecha y el PIN.
//!
//! Las fechas se escriben **relativas a hoy**, para que el test diga lo mismo
//! cualquier día que se corra.
//!
//! [ADR-0023]: ../../../docs/adr/0023-vencimientos.md

use obrain_api::{Brain, Error, indexar};
use obrain_core::{EntityId, Store, TimePoint};
use obrain_expiry::Fecha;
use obrain_ocr::Ocr;
use std::path::{Path, PathBuf};

fn hoy() -> Fecha {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    Fecha::hoy(ms)
}

fn dentro_de(dias: i64) -> Fecha {
    Fecha::desde_dias(hoy().dias() + dias)
}

const MESES: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];

/// «30 de septiembre de 2026».
fn larga(f: Fecha) -> String {
    format!("{} de {} de {}", f.d, MESES[f.m as usize - 1], f.y)
}

/// «15/11/2026».
fn corta(f: Fecha) -> String {
    format!("{:02}/{:02}/{}", f.d, f.m, f.y)
}

/// El OCR de juguete: la foto de una TIE que caduca en 53 días, con su NIE
/// —la puerta la vuelve sensible por su contenido—.
struct Tie;

impl Ocr for Tie {
    fn engine(&self) -> String {
        "juguete/es-ES".into()
    }
    fn image(&self, _bytes: &[u8]) -> Result<String, String> {
        let f = dentro_de(53);
        Ok(format!(
            "TARJETA DE IDENTIDAD DE EXTRANJERO\nNIE X1234567L\nVALIDEZ {:02} {:02} {}",
            f.d, f.m, f.y
        ))
    }
}

fn boveda(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("obrain-api-venc-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let b = d.join("Bóveda");
    let escribir = |ruta: &str, texto: &str| {
        let p = b.join(ruta);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, texto).unwrap();
    };
    std::fs::create_dir_all(b.join("01 Identidad")).unwrap();
    image::RgbImage::from_pixel(48, 30, image::Rgb([150, 200, 170]))
        .save(b.join("01 Identidad").join("IMG_2291.png"))
        .unwrap();
    escribir(
        "02 Salud/Tarjeta sanitaria.txt",
        &format!(
            "Tarjeta sanitaria individual. Válida hasta el {}.",
            corta(dentro_de(-12))
        ),
    );
    escribir(
        "02 Salud/Póliza salud.txt",
        &format!(
            "Póliza de salud.\n\nPeriodo de cobertura: desde el {} hasta el {}.",
            larga(dentro_de(-23)),
            larga(dentro_de(342))
        ),
    );
    escribir(
        "05 Vivienda/Contrato de alquiler.txt",
        &format!(
            "El contrato vence el {} y se renueva por un año.",
            larga(dentro_de(7))
        ),
    );
    // Lo único normal de la bóveda: ni su nombre ni su texto son sensibles
    // (la puerta ve «contrato», «póliza» y «tarjeta sanitaria» en los otros).
    escribir(
        "07 Trabajo/Curso de Java.txt",
        &format!(
            "Acceso al curso de Java válido hasta el {}.",
            corta(dentro_de(30))
        ),
    );
    escribir(
        "03 Estudios/Notas.txt",
        "Apuntes de SQL con Oracle. Sin fechas.",
    );
    b
}

fn indexar(store: &mut Store, b: &Path) -> obrain_api::IndexReport {
    indexar::index_with(store, &[b.to_path_buf()], None, Some(&Tie)).unwrap()
}

fn id_de(b: &Brain, nombre: &str) -> EntityId {
    let hit = b
        .search(nombre, 5)
        .unwrap()
        .hits
        .into_iter()
        .find(|h| h.label.contains(nombre))
        .unwrap_or_else(|| panic!("no encuentro «{nombre}»"));
    EntityId(hit.entity)
}

/// `(nombre, días, carpeta)` de cada fila de la tarjeta.
fn tarjeta(b: &Brain) -> Vec<(String, Option<i64>, String)> {
    b.expiring()
        .unwrap()
        .into_iter()
        .map(|e| (e.row.label, e.days, e.folder.unwrap_or_default()))
        .collect()
}

#[test]
fn la_tarjeta_ensena_lo_caducado_y_lo_de_los_proximos_90_dias() {
    let b_dir = boveda("tarjeta");
    let mut store = Store::open_in_memory().unwrap();
    let r = indexar(&mut store, &b_dir);
    assert_eq!(r.expiring, 5, "{r:?}");
    let b = Brain::from_store(store);

    // Lo caducado arriba; la póliza (en 342 días) todavía no; las notas no
    // tienen fecha.
    assert_eq!(
        tarjeta(&b),
        [
            ("Tarjeta sanitaria.txt".into(), Some(-12), "02 Salud".into()),
            (
                "Contrato de alquiler.txt".into(),
                Some(7),
                "05 Vivienda".into()
            ),
            ("Curso de Java.txt".into(), Some(30), "07 Trabajo".into()),
            ("IMG_2291.png".into(), Some(53), "01 Identidad".into()),
        ]
    );
    let tie = &b.expiring().unwrap()[3];
    assert_eq!(tie.row.sensitivity, "sensitive");
    assert_eq!(tie.date.as_deref(), Some(dentro_de(53).iso().as_str()));
    assert!(!tie.locked && !tie.by_user);

    // En la ficha, con su «¿por qué?»: una dirección, no el texto.
    let id = EntityId(tie.row.id.clone());
    let ficha = b.entity(&id).unwrap();
    let c = ficha
        .inferred
        .iter()
        .find(|c| c.predicate == "expires_on")
        .expect("la fecha está en lo inferido");
    assert_eq!(c.value, dentro_de(53).iso());
    assert_eq!(c.label, "caduca el");
    let ev = c.evidence.as_deref().unwrap();
    assert!(ev.starts_with("regla=validez fragmento=0 bytes="), "{ev}");
    assert!(!ev.contains("X1234567L"));
}

#[test]
fn corregir_gana_y_reindexar_no_lo_pisa() {
    let b_dir = boveda("corregir");
    // En archivo: «actualizar» indexa con otra conexión a la misma base,
    // como hace la aplicación.
    let db = b_dir.with_file_name("brain.obrain");
    let mut store = Store::open(&db).unwrap();
    indexar(&mut store, &b_dir);
    let b = Brain::from_store(store);
    let sanitaria = id_de(&b, "sanitaria");
    let poliza = id_de(&b, "Póliza");

    // «Ya la renové»: no caduca. Y la póliza, que el usuario sabe que vence
    // antes.
    b.set_expiry(&sanitaria, None).unwrap();
    b.set_expiry(&poliza, Some(&dentro_de(10).iso())).unwrap();
    assert!(matches!(
        b.set_expiry(&poliza, Some("31/02/2027")),
        Err(Error::FechaInvalida(_))
    ));
    assert!(matches!(
        b.set_expiry(&poliza, Some("2027-02-31")),
        Err(Error::FechaInvalida(_))
    ));

    // En la ficha, una sola fecha: la tuya. La inferida sigue en el log.
    let ficha = b.entity(&poliza).unwrap();
    let fechas: Vec<_> = ficha
        .observed
        .iter()
        .chain(&ficha.inferred)
        .filter(|c| c.predicate == "expires_on")
        .map(|c| (c.value.clone(), c.by_user))
        .collect();
    assert_eq!(fechas, [(dentro_de(10).iso(), true)]);
    let en_el_log = b
        .store()
        .query(
            Some(&poliza),
            Some("expires_on"),
            TimePoint::at(i64::MAX / 2),
        )
        .unwrap()
        .len();
    assert_eq!(en_el_log, 2, "la del usuario y la inferida que corrigió");

    let esperada = vec![
        (
            "Contrato de alquiler.txt".to_string(),
            Some(7),
            "05 Vivienda".to_string(),
        ),
        ("Póliza salud.txt".into(), Some(10), "02 Salud".into()),
        ("Curso de Java.txt".into(), Some(30), "07 Trabajo".into()),
        ("IMG_2291.png".into(), Some(53), "01 Identidad".into()),
    ];
    assert_eq!(tarjeta(&b), esperada);
    assert!(b.expiring().unwrap()[1].by_user);

    // Reindexar —«actualizar»— no pisa ninguna de las dos.
    let mut otra = Store::open(&db).unwrap();
    let r = indexar(&mut otra, &b_dir);
    assert_eq!(r.expiring, 5, "el detector sigue viendo sus cinco fechas");
    assert_eq!(tarjeta(&b), esperada);
}

#[test]
fn con_el_pin_bloqueado_lo_sensible_sale_sin_fecha() {
    let b_dir = boveda("pin");
    let mut store = Store::open_in_memory().unwrap();
    indexar(&mut store, &b_dir);
    let b = Brain::from_store(store);
    b.set_pin("2468", None).unwrap();
    b.lock();

    // La TIE (por su NIE), el contrato y la tarjeta sanitaria (por su nombre)
    // son sensibles: salen al final, por nombre, con su carpeta y sin fecha.
    // Lo normal sigue diciendo la suya.
    let v = b.expiring().unwrap();
    let filas: Vec<_> = v
        .iter()
        .map(|e| (e.row.label.as_str(), e.days, e.locked))
        .collect();
    assert_eq!(
        filas,
        [
            ("Curso de Java.txt", Some(30), false),
            ("Contrato de alquiler.txt", None, true),
            ("IMG_2291.png", None, true),
            ("Tarjeta sanitaria.txt", None, true),
        ]
    );
    let tie = &v[2];
    assert_eq!(tie.folder.as_deref(), Some("01 Identidad"));
    assert_eq!((tie.date.as_deref(), tie.by_user), (None, false));

    // La ficha tampoco la dice, y no se puede corregir lo que no se ve.
    let id = EntityId(tie.row.id.clone());
    let ficha = b.entity(&id).unwrap();
    assert!(
        !ficha
            .inferred
            .iter()
            .chain(&ficha.observed)
            .any(|c| c.predicate == "expires_on")
    );
    assert!(matches!(b.set_expiry(&id, None), Err(Error::Bloqueado)));

    // Desbloqueado, todo vuelve.
    b.unlock("2468").unwrap();
    let v = b.expiring().unwrap();
    assert_eq!(v[3].days, Some(53));
    assert!(v.iter().all(|e| !e.locked && e.date.is_some()));
}
