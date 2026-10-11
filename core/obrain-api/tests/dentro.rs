//! Un documento dentro de otro, de punta a punta ([ADR-0030]): `indexar` una
//! bóveda con un paquete, lo que se ve en la vista previa y en la búsqueda, y
//! la sensibilidad que el paquete hereda de lo que lleva dentro.
//!
//! [ADR-0030]: ../../../docs/adr/0030-un-documento-dentro-de-otro.md

use obrain_api::{Brain, indexar};
use obrain_core::{EntityId, Sensitivity, Store, TimePoint};
use std::path::{Path, PathBuf};

/// Un texto de `n` párrafos sobre `tema`, como los de un estado de cuenta.
fn texto(tema: &str, n: usize) -> String {
    (0..n)
        .map(|i| format!("{tema}, movimiento {i}: pago con referencia y su importe del periodo."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn marzo() -> String {
    texto("Marzo", 5)
}

fn abril() -> String {
    texto("Abril", 6)
}

fn caratula() -> String {
    texto("Carátula del documento de viaje", 4)
}

/// La bóveda: los estados de marzo y abril sueltos, el expediente que los
/// junta (con su portada y la carátula), la carátula en su carpeta y unas
/// notas que no tienen nada que ver.
fn boveda(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("obrain-api-dentro-{nombre}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let b = d.join("Bóveda");
    let escribir = |carpeta: &str, archivo: &str, texto: &str| {
        let c = b.join(carpeta);
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(c.join(archivo), texto).unwrap();
    };
    escribir("05 Banco", "Marzo.txt", &marzo());
    escribir("05 Banco", "Abril.txt", &abril());
    escribir("01 Escaneos", "Caratula.txt", &caratula());
    escribir(
        "04 Papeles",
        "Expediente consulado.txt",
        &format!(
            "{}\n\n{}\n\n{}\n\n{}",
            texto("Portada del expediente", 2),
            caratula(),
            marzo(),
            abril()
        ),
    );
    escribir(
        "03 Estudios",
        "Notas.txt",
        "Apuntes de SQL con Oracle, sin movimientos.",
    );
    b
}

fn indexar(store: &mut Store, b: &Path) -> obrain_api::IndexReport {
    indexar::index_with(store, &[b.to_path_buf()], None, None).unwrap()
}

fn id_de(b: &Brain, nombre: &str) -> EntityId {
    id_de_por(b, nombre, nombre)
}

/// El documento que se llama `nombre`, buscándolo por algo que dice.
fn id_de_por(b: &Brain, consulta: &str, nombre: &str) -> EntityId {
    let hit = b
        .search(consulta, 20)
        .unwrap()
        .hits
        .into_iter()
        .find(|h| h.label.contains(nombre))
        .unwrap_or_else(|| panic!("no encuentro «{nombre}»"));
    EntityId(hit.entity)
}

#[test]
fn el_paquete_dice_lo_que_lleva_y_cada_uno_donde_esta() {
    let dir = boveda("ficha");
    let mut store = Store::open_in_memory().unwrap();
    let r = indexar(&mut store, &dir);
    assert_eq!(r.inside, 3, "carátula, marzo y abril: {r:?}");
    let b = Brain::from_store(store);
    let expediente = id_de(&b, "Expediente");

    // En la ficha del paquete, lo que lleva, en el orden de su texto.
    let ficha = b.entity(&expediente).unwrap();
    let nombres: Vec<&str> = ficha.parts.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(nombres, ["Caratula.txt", "Marzo.txt", "Abril.txt"]);
    let m = &ficha.parts[1];
    assert_eq!((m.paragraphs, m.of), (5, 5));

    // Y el tramo apunta a marzo **en el texto del paquete**: la vista previa
    // abierta por ahí lo marca entero.
    let [x, y] = m.span.unwrap();
    let v = b.preview_at(&expediente, None, Some((x, y))).unwrap();
    let (s, e) = v.highlight.unwrap();
    assert_eq!(&v.text.unwrap()[s..e], marzo());

    // En la ficha de marzo: «está dentro de», a donde lleva el mismo tramo.
    let marzo_id = id_de(&b, "Marzo");
    let ficha = b.entity(&marzo_id).unwrap();
    let c = ficha
        .observed
        .iter()
        .find(|c| c.predicate == "part_of")
        .expect("marzo sabe dónde está");
    assert_eq!(c.label, "está dentro de");
    assert_eq!(c.value, "Expediente consulado.txt");
    assert_eq!(c.value_entity.as_deref(), Some(expediente.as_str()));
    assert_eq!(c.target_span, Some([x, y]));
    assert!(c.span.is_none(), "el tramo no es del texto de marzo");
    assert!(
        !c.evidence.as_deref().unwrap().contains("Marzo"),
        "ni una palabra al log"
    );

    // Las notas no contienen ni están en nada.
    let notas = b.entity(&id_de_por(&b, "Oracle", "Notas")).unwrap();
    assert!(notas.parts.is_empty());
    assert!(!notas.observed.iter().any(|c| c.predicate == "part_of"));
}

/// Cinco filas que son tres documentos se leen como tres: lo de dentro,
/// justo debajo de su paquete. Ninguna fila se esconde.
#[test]
fn en_la_busqueda_lo_de_dentro_va_debajo_de_su_paquete() {
    let dir = boveda("busqueda");
    let mut store = Store::open_in_memory().unwrap();
    indexar(&mut store, &dir);
    let b = Brain::from_store(store);
    let expediente = id_de(&b, "Expediente");

    let hits = b.search("movimiento pago referencia", 10).unwrap().hits;
    let filas: Vec<(&str, Option<&str>)> = hits
        .iter()
        .map(|h| (h.label.as_str(), h.inside.as_deref()))
        .collect();
    let i = filas
        .iter()
        .position(|(l, _)| *l == "Expediente consulado.txt")
        .expect("el paquete sale");
    let debajo: Vec<&str> = filas[i + 1..]
        .iter()
        .take_while(|(_, dentro)| *dentro == Some(expediente.as_str()))
        .map(|(l, _)| *l)
        .collect();
    let mut debajo_ordenado = debajo.clone();
    debajo_ordenado.sort();
    assert_eq!(
        debajo_ordenado,
        ["Abril.txt", "Caratula.txt", "Marzo.txt"],
        "{filas:?}"
    );
    assert!(
        filas
            .iter()
            .all(|(l, dentro)| dentro.is_none() || debajo.contains(l)),
        "nadie más va metido: {filas:?}"
    );
    assert_eq!(filas.len(), 4, "nada se esconde: {filas:?}");
}

/// El caso que lo pidió: la carátula está en una carpeta que marcas y el
/// expediente, fuera, la lleva dentro. Se tapa en el acto, dice por qué, y
/// la pasada siguiente no lo destapa.
#[test]
fn el_paquete_hereda_la_sensibilidad_de_lo_que_lleva() {
    let dir = boveda("sensible");
    let db = dir.with_file_name("brain.obrain");
    let mut store = Store::open(&db).unwrap();
    indexar(&mut store, &dir);
    let mut b = Brain::from_store(store);
    let expediente = id_de(&b, "Expediente");
    assert_eq!(b.entity(&expediente).unwrap().sensitivity, "normal");

    let at = TimePoint::at(i64::MAX / 2);
    let escaneos =
        obrain_api::marks::folder_at(b.store(), &dir.join("01 Escaneos").to_string_lossy(), at)
            .unwrap()
            .expect("la carpeta está en el log");
    b.mark_folder(&escaneos, Some(Sensitivity::Sensitive))
        .unwrap();

    let ficha = b.entity(&expediente).unwrap();
    assert_eq!(ficha.sensitivity, "sensitive", "se tapa en el acto");
    assert!(ficha.protection.is_none(), "no es por su carpeta");
    let porque = ficha.by_part.expect("es por lo que lleva");
    assert_eq!(porque.label, "Caratula.txt");
    assert_eq!(porque.because, "folder");
    assert_eq!(porque.folder.unwrap().label, "01 Escaneos");
    // Lo de dentro no se contagia del paquete: marzo sigue normal.
    assert_eq!(b.entity(&id_de(&b, "Marzo")).unwrap().sensitivity, "normal");

    // La pasada siguiente (otra conexión, como «actualizar») no lo destapa.
    let mut otra = Store::open(&db).unwrap();
    let r = indexar(&mut otra, &dir);
    assert_eq!(r.from_parts, 1, "{r:?}");
    assert_eq!(r.inside, 3, "y no se duplica nada");
    assert_eq!(b.entity(&expediente).unwrap().sensitivity, "sensitive");
    let hit = b
        .search("Portada del expediente", 5)
        .unwrap()
        .hits
        .into_iter()
        .find(|h| h.entity == expediente.as_str())
        .unwrap();
    assert_eq!(hit.sensitivity, "sensitive");

    // Quitar la marca lo destapa.
    b.mark_folder(&escaneos, None).unwrap();
    assert_eq!(b.entity(&expediente).unwrap().sensitivity, "normal");
}

#[test]
fn reindexar_no_repite_ni_pierde_nada() {
    let dir = boveda("reindexar");
    let mut store = Store::open_in_memory().unwrap();
    indexar(&mut store, &dir);
    let antes = store.log_len().unwrap();
    let r = indexar(&mut store, &dir);
    assert_eq!(r.inside, 3);
    assert_eq!(
        store.log_len().unwrap(),
        antes,
        "la segunda pasada no escribe"
    );
}

/// ADR-0032, el hueco que destapó el pendiente: la carátula está dos veces
/// con el mismo texto, una copia vieja en `04 Papeles` y la tuya en una
/// carpeta que marcas, más nueva. Antes sólo contaba la vieja (el original)
/// y el expediente salía **sin tapar**. Ahora cuentan las dos: se tapa por la
/// marcada, la copia dice «está dentro de», y el expediente no dice
/// «contiene: carátula, carátula».
#[test]
fn una_copia_en_una_carpeta_marcada_tambien_tapa_el_paquete() {
    let dir = boveda("copias");
    let vieja = dir.join("04 Papeles").join("Caratula vieja.txt");
    std::fs::write(&vieja, caratula()).unwrap();
    // La de `01 Escaneos` es la nueva: la vieja se guardó hace un mes.
    let mes = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
    std::fs::File::options()
        .write(true)
        .open(&vieja)
        .unwrap()
        .set_modified(mes)
        .unwrap();

    let mut store = Store::open_in_memory().unwrap();
    let r = indexar(&mut store, &dir);
    assert_eq!(r.inside, 4, "carátula (dos copias), marzo y abril: {r:?}");
    let mut b = Brain::from_store(store);
    let expediente = id_de(&b, "Expediente");

    let at = TimePoint::at(i64::MAX / 2);
    let escaneos =
        obrain_api::marks::folder_at(b.store(), &dir.join("01 Escaneos").to_string_lossy(), at)
            .unwrap()
            .unwrap();
    b.mark_folder(&escaneos, Some(Sensitivity::Sensitive))
        .unwrap();

    let ficha = b.entity(&expediente).unwrap();
    assert_eq!(ficha.sensitivity, "sensitive", "la copia marcada lo tapa");
    let porque = ficha.by_part.expect("es por lo que lleva");
    assert_eq!(porque.label, "Caratula.txt");
    assert_eq!(porque.folder.unwrap().label, "01 Escaneos");

    // «Contiene»: una fila por texto, con la copia en ella.
    let filas: Vec<(&str, usize)> = ficha
        .parts
        .iter()
        .map(|p| (p.label.as_str(), p.copies.len()))
        .collect();
    assert_eq!(
        filas,
        [("Caratula.txt", 1), ("Marzo.txt", 0), ("Abril.txt", 0)]
    );
    assert_eq!(ficha.parts[0].copies[0].label, "Caratula vieja.txt");

    // Y cada copia dice dónde está, con el mismo tramo.
    for nombre in ["Caratula.txt", "Caratula vieja.txt"] {
        let id = b
            .search("Carátula documento de viaje", 10)
            .unwrap()
            .hits
            .into_iter()
            .find(|h| h.label == nombre)
            .map(|h| EntityId(h.entity))
            .unwrap_or_else(|| panic!("no sale {nombre}"));
        let c = b
            .entity(&id)
            .unwrap()
            .observed
            .into_iter()
            .find(|c| c.predicate == "part_of")
            .unwrap_or_else(|| panic!("{nombre} no dice «está dentro de»"));
        assert_eq!(c.value_entity.as_deref(), Some(expediente.as_str()));
        assert_eq!(c.target_span, Some(ficha.parts[0].span.unwrap()));
    }
}
