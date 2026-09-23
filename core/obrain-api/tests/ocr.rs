//! El OCR de punta a punta: `indexar` una carpeta con la foto de una TIE.
//!
//! El reconocedor es de juguete —el de verdad es el de Windows—, pero todo lo
//! demás es el pipeline real: escanear, enlazar, extraer, clasificar.

use obrain_api::{indexar, Brain};
use obrain_core::{Sensitivity, Store};
use obrain_ocr::Ocr;

/// Lo que diría el reconocedor de la foto de una TIE. El NIE tiene su letra
/// de control bien puesta: la puerta lo reconoce por el contenido.
struct Tie;

impl Ocr for Tie {
    fn engine(&self) -> String {
        "juguete/es-ES".into()
    }
    fn image(&self, _bytes: &[u8]) -> Result<String, String> {
        Ok("TARJETA DE IDENTIDAD DE EXTRANJERO\nEJEMPLO MUESTRA JUAN PABLO\nNIE X1234567L".into())
    }
}

#[test]
fn la_foto_de_la_tie_se_encuentra_se_ve_y_sale_sensible_sin_marcar_nada() {
    let d = std::env::temp_dir().join(format!("obrain-api-ocr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let boveda = d.join("Bóveda");
    std::fs::create_dir_all(boveda.join("Fotos")).unwrap();
    // Una foto de verdad, decodificable, para que la miniatura también salga.
    image::RgbImage::from_pixel(48, 30, image::Rgb([150, 200, 170]))
        .save(boveda.join("Fotos").join("IMG_2291.png"))
        .unwrap();

    let mut store = Store::open_in_memory().unwrap();
    let r = indexar::index_with(&mut store, std::slice::from_ref(&boveda), None, Some(&Tie)).unwrap();
    assert_eq!((r.documents_read, r.ocr), (1, 1), "{r:?}");
    assert_eq!(r.sensitive, 1, "el NIE reconocido la vuelve sensible");

    let b = Brain::from_store(store);
    let hits = b.search("extranjero", 5).unwrap().hits;
    let foto = hits.first().expect("la foto se encuentra por lo que dice");
    assert!(foto.label.contains("IMG_2291"));
    let id = obrain_core::EntityId(foto.entity.clone());
    assert_eq!(b.store().sensitivity(&id).unwrap(), Sensitivity::Sensitive);

    // La vista previa sigue siendo la foto, con el texto reconocido debajo.
    let v = b.preview(&id, Some("extranjero")).unwrap();
    assert_eq!(v.kind, "image");
    assert!(v.image.is_some());
    assert!(v.text.as_deref().is_some_and(|t| t.contains("EXTRANJERO")));

    // Y el porqué: quién leyó el texto.
    let ficha = b.entity(&id).unwrap();
    assert!(ficha.observed.iter().any(|c| c.predicate == "ocr_engine" && c.value == "juguete/es-ES"));
}
