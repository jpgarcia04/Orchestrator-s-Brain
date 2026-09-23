//! OCR en M2: fotos y PDF escaneados, con un reconocedor de juguete.
//!
//! El reconocedor de verdad es el de Windows (`obrain-ocr`) y aquí no existe.
//! Lo que se prueba es lo que hace el pipeline con él —cuándo lo llama, qué
//! anota en el log, que no repite trabajo—, igual que los tests de M4 prueban
//! la puerta con un embebedor de juguete.

use obrain_core::{EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};
use obrain_extract::{extract_all, extract_all_with, index, load_files, search, ExtractReport};
use obrain_ocr::Ocr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(nombre: &str) -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("obrain-ocr-{nombre}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Lo que deja M1 en el log para un archivo local.
fn ingerir(store: &Store, path: &Path, at: i64) -> EntityId {
    let bytes = std::fs::read(path).unwrap();
    let e = store.create_entity(at).unwrap();
    let w = |p: &str, v: Value| {
        store.assert(NewAssertion::new(e.clone(), p, v, Method::Deterministic, at).source("scan@m1")).unwrap();
    };
    w("has_facet", Facet::Addressable.as_str().into());
    w("path_observed_at", path.to_string_lossy().to_string().into());
    w("file_size", Value::Number(bytes.len() as f64));
    w("availability", "local".into());
    w("has_hash", blake3::hash(&bytes).to_hex().to_string().into());
    e
}

/// Reconoce siempre lo mismo y cuenta cuántas veces se le llamó.
struct Juguete {
    texto: &'static str,
    llamadas: AtomicUsize,
}

impl Juguete {
    fn new(texto: &'static str) -> Self {
        Juguete { texto, llamadas: AtomicUsize::new(0) }
    }
    fn llamadas(&self) -> usize {
        self.llamadas.load(Ordering::Relaxed)
    }
}

impl Ocr for Juguete {
    fn engine(&self) -> String {
        "juguete/es-ES".into()
    }
    fn image(&self, _bytes: &[u8]) -> Result<String, String> {
        self.llamadas.fetch_add(1, Ordering::Relaxed);
        Ok(self.texto.to_string())
    }
    fn pdf(&self, _bytes: &[u8], _max: u32) -> Option<Result<String, String>> {
        self.llamadas.fetch_add(1, Ordering::Relaxed);
        Some(Ok(self.texto.to_string()))
    }
}

fn correr(store: &mut Store, at: i64, ocr: Option<&dyn Ocr>) -> ExtractReport {
    let files = load_files(store, TimePoint::at(at)).unwrap();
    let mut r = ExtractReport::default();
    store.transaction(|s| extract_all_with(s, &files, at, &mut r, ocr)).unwrap();
    assert!(r.arithmetic_holds(), "la aritmética del informe no cierra: {r:?}");
    r
}

/// Un PDF de una página **sin texto**: lo que sale de un escáner, sin la
/// imagen (al lector de PDF le da igual lo que haya pintado).
fn pdf_escaneado() -> Vec<u8> {
    let objetos = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> >>",
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, o) in objetos.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objetos.len() + 1));
    for o in offsets {
        pdf.push_str(&format!("{o:010} 00000 n \n"));
    }
    pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objetos.len() + 1));
    pdf.into_bytes()
}

const TIE: &str = "TARJETA DE IDENTIDAD DE EXTRANJERO\nNIE X1234567L\nVALIDEZ 15 11 2026";

// ═════════════════════════════════════════════════════════════════════ fotos

/// Sin reconocedor, una foto es lo que era: un formato que no se lee.
#[test]
fn sin_ocr_una_foto_se_salta_como_siempre() {
    let dir = temp_dir("sin");
    let mut store = Store::open_in_memory().unwrap();
    std::fs::write(dir.join("IMG_2291.jpg"), b"\xFF\xD8\xFFfoto").unwrap();
    ingerir(&store, &dir.join("IMG_2291.jpg"), 1);

    let r = correr(&mut store, 2, None);
    assert_eq!(r.skipped_unsupported, 1);
    assert_eq!(r.opened, 0, "sin OCR no se abre la foto");
    // Y la función de siempre sigue igual.
    let files = load_files(&store, TimePoint::at(3)).unwrap();
    let mut r2 = ExtractReport::default();
    extract_all(&store, &files, 3, &mut r2).unwrap();
    assert_eq!(r2.skipped_unsupported, 1);
}

/// El caso de la Fase 3: la foto de la TIE se lee, se busca por lo que dice,
/// y el log anota que ese texto lo reconoció un OCR.
#[test]
fn con_ocr_la_foto_de_la_tie_se_lee_y_se_busca() {
    let dir = temp_dir("tie");
    let mut store = Store::open_in_memory().unwrap();
    std::fs::write(dir.join("IMG_2291.jpg"), b"\xFF\xD8\xFFfoto").unwrap();
    let tie = ingerir(&store, &dir.join("IMG_2291.jpg"), 1);
    let ocr = Juguete::new(TIE);

    let r = correr(&mut store, 2, Some(&ocr));
    assert_eq!((r.extracted, r.ocr), (1, 1));
    assert_eq!(ocr.llamadas(), 1);

    let hits = search(&store, "extranjero", 5, TimePoint::at(3)).unwrap();
    assert_eq!(hits.first().map(|h| &h.entity), Some(&tie));

    let tp = TimePoint::at(3);
    let motor = store.resolve(&tie, "ocr_engine", tp).unwrap().expect("tiene que decir quién lo leyó");
    assert_eq!(motor.object.as_text(), Some("juguete/es-ES"));
    assert_eq!(
        store.resolve(&tie, "extraction_status", tp).unwrap().unwrap().object.as_text(),
        Some("ok")
    );

    // Otra pasada no vuelve a llamar al reconocedor: el archivo no cambió.
    let r2 = correr(&mut store, 4, Some(&ocr));
    assert_eq!(r2.unchanged, 1);
    assert_eq!(ocr.llamadas(), 1);
}

/// Una foto sin nada legible sale vacía, no con un error.
#[test]
fn una_foto_sin_texto_sale_vacia() {
    let dir = temp_dir("vacia");
    let mut store = Store::open_in_memory().unwrap();
    std::fs::write(dir.join("atardecer.png"), b"\x89PNGfoto").unwrap();
    let e = ingerir(&store, &dir.join("atardecer.png"), 1);
    let r = correr(&mut store, 2, Some(&Juguete::new("   \n  ")));
    assert_eq!((r.empty, r.ocr), (1, 0));
    assert!(store.resolve(&e, "ocr_engine", TimePoint::at(3)).unwrap().is_none());
}

// ══════════════════════════════════════════════════════════ PDF escaneados

/// Un PDF escaneado: sin OCR sale vacío (o falla); cuando hay OCR se vuelve a
/// mirar **una vez**, se lee, y ya no se repite.
#[test]
fn un_pdf_escaneado_se_reintenta_una_vez_cuando_hay_ocr() {
    let dir = temp_dir("pdf");
    let mut store = Store::open_in_memory().unwrap();
    std::fs::write(dir.join("Pasaporte (escaneo).pdf"), pdf_escaneado()).unwrap();
    let pas = ingerir(&store, &dir.join("Pasaporte (escaneo).pdf"), 1);

    let r = correr(&mut store, 2, None);
    assert_eq!(r.extracted, 0, "sin OCR no hay texto: {r:?}");

    let ocr = Juguete::new("PASAPORTE\nESTADOS UNIDOS MEXICANOS\nFECHA DE CADUCIDAD 12 03 2031");
    let r = correr(&mut store, 3, Some(&ocr));
    assert_eq!((r.extracted, r.ocr), (1, 1), "{r:?}");
    assert_eq!(ocr.llamadas(), 1);
    assert!(!search(&store, "caducidad", 5, TimePoint::at(4)).unwrap().is_empty());
    assert_eq!(
        store.resolve(&pas, "ocr_engine", TimePoint::at(4)).unwrap().unwrap().object.as_text(),
        Some("juguete/es-ES")
    );
    let prev = index::previous(
        store.conn(),
        &pas,
        obrain_extract::chunk::CHUNKER,
        obrain_extract::chunk::CHUNKER_VERSION,
    )
    .unwrap()
    .unwrap();
    assert_eq!(prev.extractor, "pdf+ocr");

    let r = correr(&mut store, 5, Some(&ocr));
    assert_eq!(r.unchanged, 1);
    assert_eq!(ocr.llamadas(), 1, "no se vuelve a reconocer lo que no cambió");
}

/// Un PDF con texto de verdad no pasa por el OCR.
#[test]
fn un_pdf_con_texto_no_llama_al_ocr() {
    let dir = temp_dir("texto");
    let mut store = Store::open_in_memory().unwrap();
    std::fs::write(dir.join("notas.txt"), "una receta de ibuprofeno").unwrap();
    ingerir(&store, &dir.join("notas.txt"), 1);
    let ocr = Juguete::new("nada");
    let r = correr(&mut store, 2, Some(&ocr));
    assert_eq!((r.extracted, r.ocr), (1, 0));
    assert_eq!(ocr.llamadas(), 0);
}
