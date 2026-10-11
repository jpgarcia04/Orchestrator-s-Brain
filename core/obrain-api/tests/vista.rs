//! La previsualización de M5b, contra archivos de verdad en disco.
//!
//! El primero de este archivo es el que importa más que ninguno, y es el mismo
//! que abre los tests de M2: **nunca se abre un archivo que no está local.**
//! Allí lo que estaba en juego era el texto; aquí es una miniatura, que es peor
//! —una imagen se genera al seleccionar una fila, y en el explorador eso pasa
//! con cada pulsación de la flecha—. Un fallo aquí no descarga un archivo:
//! descarga la carpeta entera mientras alguien la recorre.
//!
//! La forma del test es deliberada y copiada de M2: el archivo **existe de
//! verdad y se puede decodificar perfectamente**. Lo único que dice que no se
//! puede tocar es su etiqueta de disponibilidad. Si la miniatura aparece, es
//! que alguien abrió un puntero de OneDrive.

use obrain_api::{Brain, preview};
use obrain_core::{EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const T: i64 = 1_700_000_000_000;

static N: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(nombre: &str) -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("obrain-vista-{nombre}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Una imagen real, decodificable y con el tamaño que se le pida.
fn imagen(dir: &Path, nombre: &str, ancho: u32, alto: u32) -> PathBuf {
    let p = dir.join(nombre);
    let mut img = image::RgbImage::new(ancho, alto);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
    }
    img.save(&p).unwrap();
    p
}

/// Lo que deja M1 en el log para un archivo.
fn ingerir(store: &Store, path: &Path, availability: &str) -> EntityId {
    let bytes = std::fs::read(path).unwrap_or_default();
    let e = store.create_entity(T).unwrap();
    let w = |p: &str, v: Value| {
        store
            .assert(NewAssertion::new(e.clone(), p, v, Method::Deterministic, T).source("scan@m1"))
            .unwrap();
    };
    w("has_facet", Facet::Addressable.as_str().into());
    w(
        "path_observed_at",
        path.to_string_lossy().to_string().into(),
    );
    w("file_size", Value::Number(bytes.len() as f64));
    w("availability", availability.into());
    e
}

fn base() -> Store {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    store
}

/// Corre M2 de verdad sobre lo que hay en el log.
fn extraer(store: &mut Store) {
    let files = obrain_extract::load_files(store, TimePoint::at(T + 1)).unwrap();
    let mut r = obrain_extract::ExtractReport::default();
    store
        .transaction(|s| obrain_extract::extract_all(s, &files, T + 2, &mut r))
        .unwrap();
}

// ═══════════════════════════════════════════════ el invariante que hereda M5b

/// **El test que no puede fallar nunca.**
///
/// La imagen está en disco, es válida y se decodifica sin problema. Lo único
/// que la protege es la etiqueta `cloud_dehydrated` que escribió M1. Si la
/// miniatura sale, la previsualización acaba de descargar un archivo de la nube
/// del usuario sin pedirle permiso — y en su corpus real hay 14 archivos así,
/// que son exactamente sus 14 documentos de Office.
#[test]
fn nunca_se_abre_una_imagen_que_no_esta_local() {
    let dir = temp_dir("nube");
    let store = base();
    let p = imagen(&dir, "captura.png", 40, 30);
    let e = ingerir(&store, &p, "cloud_dehydrated");

    let v = Brain::from_store(store).preview(&e, None).unwrap();

    assert_eq!(
        v.kind, "none",
        "generó algo de un archivo que está en la nube"
    );
    assert!(v.image.is_none());
    assert!(!v.readable, "la guarda de M1 dijo que sí a un puntero");
    // §4.6: y se dice por qué, con las palabras que le sirven a quien mira.
    let porque = v.reason.expect("«no lo sé» es una pantalla, no un hueco");
    assert!(porque.contains("nube"), "{porque}");
    assert!(porque.contains("descargaría"), "{porque}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `Availability::parse` devuelve `None` ante un valor que no reconoce, y ese
/// `None` **no es lo mismo que local**. Ante la duda no se toca el archivo.
#[test]
fn una_disponibilidad_desconocida_tampoco_se_abre() {
    let dir = temp_dir("desconocida");
    let store = base();
    let p = imagen(&dir, "foto.png", 40, 30);
    let e = ingerir(&store, &p, "un_estado_que_no_existe_todavia");

    let v = Brain::from_store(store).preview(&e, None).unwrap();
    assert_eq!(v.kind, "none");
    assert!(!v.readable);
    assert!(v.image.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

// ═════════════════════════════════════════════════════ lo que sí se puede ver

/// La región más grande del corpus real son 229 capturas de pantalla. Un panel
/// que no enseña una imagen falla justo en la parte más grande del disco.
#[test]
fn una_imagen_local_se_ve() {
    let dir = temp_dir("imagen");
    let store = base();
    let p = imagen(&dir, "captura.png", 1600, 900);
    let e = ingerir(&store, &p, "local");

    let v = Brain::from_store(store).preview(&e, None).unwrap();

    assert_eq!(v.kind, "image");
    assert!(v.readable);
    let datos = v.image.expect("hay miniatura");
    assert!(
        datos.starts_with("data:image/"),
        "{}",
        &datos[..40.min(datos.len())]
    );
    assert!(datos.contains(";base64,"));
    assert!(
        v.width <= preview::THUMB && v.height <= preview::THUMB,
        "{}×{}",
        v.width,
        v.height
    );
    // Se redujo de verdad, y guardando la proporción.
    assert_eq!(v.width, preview::THUMB);
    assert_eq!(v.height, preview::THUMB * 900 / 1600);
    assert!(v.reason.is_none(), "no hay nada que explicar cuando se ve");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Una imagen pequeña no se estira: se ve como es. Agrandarla ocuparía cien
/// veces más en el puente para enseñar exactamente lo mismo, peor.
#[test]
fn una_imagen_pequeña_no_se_agranda() {
    let dir = temp_dir("pequeña");
    let store = base();
    let p = imagen(&dir, "icono.png", 48, 48);
    let e = ingerir(&store, &p, "local");

    let v = Brain::from_store(store).preview(&e, None).unwrap();
    assert_eq!((v.width, v.height), (48, 48));

    let _ = std::fs::remove_dir_all(&dir);
}

/// **La prueba de que previsualizar texto no abre nada.**
///
/// Se extrae con M2, se borra el archivo del disco, y el texto sigue estando.
/// Sale de `chunks`, que es una proyección ([ADR-0005]) — y por eso la
/// previsualización de texto no puede hidratar un puntero de OneDrive ni por
/// descuido, sin necesidad de ninguna guarda.
#[test]
fn el_texto_se_ve_aunque_el_archivo_ya_no_este() {
    let dir = temp_dir("texto");
    let mut store = base();
    let p = dir.join("resolucion.txt");
    std::fs::write(&p, "Resolución del IMSS sobre la pensión del solicitante.").unwrap();
    let e = ingerir(&store, &p, "local");
    extraer(&mut store);

    std::fs::remove_file(&p).unwrap();
    assert!(!p.exists());

    let v = Brain::from_store(store).preview(&e, None).unwrap();
    assert_eq!(v.kind, "text");
    assert!(v.text.unwrap().contains("pensión"));
    assert!(!v.truncated);

    let _ = std::fs::remove_dir_all(&dir);
}

/// [ADR-0016] llevado al panel: la ventana se elige, no se recorta por el
/// principio. Si la respuesta está más allá del presupuesto, el documento se
/// abre **donde está la respuesta** y se dice dónde empezó.
#[test]
fn con_consulta_el_documento_se_abre_donde_esta_la_respuesta() {
    let dir = temp_dir("ventana");
    let mut store = base();
    let relleno = "Texto de relleno perfectamente legible que no dice nada.\n\n".repeat(1_200);
    let p = dir.join("largo.txt");
    std::fs::write(
        &p,
        format!("{relleno}El comprobante de ingresos del solicitante.\n\n{relleno}"),
    )
    .unwrap();
    let e = ingerir(&store, &p, "local");
    extraer(&mut store);

    let brain = Brain::from_store(store);

    let sin = brain.preview(&e, None).unwrap();
    assert_eq!(sin.offset, 0);
    assert!(
        sin.truncated,
        "el documento no cabe entero, que es el caso a probar"
    );
    assert!(!sin.text.unwrap().contains("comprobante"));

    let con = brain.preview(&e, Some("comprobante de ingresos")).unwrap();
    assert!(con.offset > 0, "no movió la ventana");
    let texto = con.text.expect("hay texto");
    let (s, f) = con.highlight.expect("hay tramo marcado");
    assert!(
        texto[s..f].contains("comprobante"),
        "la marca apunta a otro sitio: {:?}",
        &texto[s..f]
    );
    // El total es del documento entero, no del tramo: decir lo contrario sería
    // fingir que lo que se ve es todo lo que hay.
    assert!(con.chars > texto.chars().count());

    let _ = std::fs::remove_dir_all(&dir);
}

// ═══════════════════════════════════════════════════ decir que no se sabe

/// Un PDF escaneado no es un error del programa, y decirlo así importa: es la
/// diferencia entre «esto está roto» y «esto necesita OCR, que es otro hito».
#[test]
fn un_archivo_sin_texto_explica_por_que() {
    let dir = temp_dir("vacio");
    let mut store = base();
    let p = dir.join("acta.txt");
    std::fs::write(&p, "   \n\n  \n").unwrap();
    let e = ingerir(&store, &p, "local");
    extraer(&mut store);

    let v = Brain::from_store(store).preview(&e, None).unwrap();
    assert_eq!(v.kind, "none");
    assert!(
        v.reason.is_some_and(|r| r.contains("escaneado")),
        "no explicó el vacío"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Una entidad que no existe es un error del llamante, no una vista en blanco
/// que se lea como un archivo sin nada dentro.
#[test]
fn una_entidad_desconocida_es_un_error() {
    let brain = Brain::from_store(base());
    assert!(brain.preview(&EntityId("no-existe".into()), None).is_err());
}
