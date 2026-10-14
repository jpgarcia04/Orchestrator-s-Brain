//! Tests de M2 contra archivos de verdad en disco.
//!
//! El primero de este archivo es el que importa más que ninguno: **nunca se
//! extrae el contenido de un archivo que no está local**. Es el mismo
//! invariante que protegió M1, y en este corpus no es teórico — los 14
//! documentos de Office de la máquina están todos deshidratados en OneDrive.

use obrain_core::{EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};
use obrain_extract::{ExtractReport, Status, chunk, extract_all, index, load_files, search};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(nombre: &str) -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "obrain-extract-{nombre}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn escribir(dir: &Path, nombre: &str, contenido: &str) -> PathBuf {
    let p = dir.join(nombre);
    std::fs::write(&p, contenido).unwrap();
    p
}

/// Simula lo que deja M1 en el log para un archivo.
fn ingerir(store: &Store, path: &Path, availability: &str, at: i64) -> EntityId {
    let bytes = std::fs::read(path).unwrap_or_default();
    let e = store.create_entity(at).unwrap();
    let w = |p: &str, v: Value| {
        store
            .assert(NewAssertion::new(e.clone(), p, v, Method::Deterministic, at).source("scan@m1"))
            .unwrap();
    };
    w("has_facet", Facet::Addressable.as_str().into());
    w(
        "path_observed_at",
        path.to_string_lossy().to_string().into(),
    );
    w("file_size", Value::Number(bytes.len() as f64));
    w("availability", availability.into());
    w("has_hash", blake3::hash(&bytes).to_hex().to_string().into());
    e
}

fn correr(store: &mut Store, at: i64) -> ExtractReport {
    let files = load_files(store, TimePoint::at(at)).unwrap();
    let mut r = ExtractReport::default();
    store
        .transaction(|s| extract_all(s, &files, at, &mut r))
        .unwrap();
    assert!(
        r.arithmetic_holds(),
        "la aritmetica del informe no cierra: {r:?}"
    );
    r
}

// ═══════════════════════════════════════════════════ el invariante de M1 y M2

/// **El test que no puede fallar nunca.**
///
/// El archivo existe en disco y tiene texto perfectamente extraíble. Lo único
/// que dice que no se puede tocar es su etiqueta de disponibilidad. Si el
/// índice acaba con un solo fragmento suyo, es que alguien abrió un placeholder
/// de OneDrive — y en la máquina real eso significa una descarga.
#[test]
fn nunca_se_extrae_un_archivo_que_no_esta_local() {
    let dir = temp_dir("nolocal");
    let mut store = Store::open_in_memory().unwrap();

    let deshidratado = escribir(
        &dir,
        "presupuesto.md",
        "Presupuesto Universidad Canada 2023.",
    );
    let placeholder = escribir(&dir, "contrato.md", "Contrato de arrendamiento firmado.");
    let local = escribir(&dir, "notas.md", "Notas locales sobre el tramite.");

    let e_desh = ingerir(&store, &deshidratado, "cloud_dehydrated", 1_000);
    let e_ph = ingerir(&store, &placeholder, "cloud_placeholder", 1_000);
    let e_local = ingerir(&store, &local, "local", 1_000);

    let r = correr(&mut store, 2_000);

    assert_eq!(r.skipped_not_local, 2, "deshidratado y placeholder");
    assert_eq!(r.opened, 1, "solo se abrio el local");
    assert_eq!(r.extracted, 1);

    for e in [&e_desh, &e_ph] {
        assert!(
            index::chunks_of(store.conn(), e).unwrap().is_empty(),
            "se extrajo texto de un archivo no local"
        );
    }
    assert!(!index::chunks_of(store.conn(), &e_local).unwrap().is_empty());

    // Y no se encuentra por su contenido, que es la comprobación de verdad:
    // si estuviera indexado, aparecería.
    let hits = search(&store, "Presupuesto Universidad", 10, TimePoint::at(3_000)).unwrap();
    assert!(
        hits.is_empty(),
        "el contenido de un placeholder llego al indice: {hits:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Falla cerrado: una etiqueta de disponibilidad corrupta o desconocida no
/// puede degradarse a "local", porque el siguiente paso es leer los bytes.
#[test]
fn una_disponibilidad_desconocida_tampoco_se_toca() {
    let dir = temp_dir("desconocida");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(&dir, "raro.md", "contenido que no debe indexarse");

    let e = store.create_entity(1_000).unwrap();
    let w = |pred: &str, v: Value| {
        store
            .assert(NewAssertion::new(
                e.clone(),
                pred,
                v,
                Method::Deterministic,
                1_000,
            ))
            .unwrap();
    };
    w("path_observed_at", p.to_string_lossy().to_string().into());
    w("file_size", Value::Number(10.0));
    w("availability", "sincronizando_quiza".into());

    let r = correr(&mut store, 2_000);
    assert_eq!(r.skipped_not_local, 1);
    assert_eq!(r.opened, 0);
    assert!(index::chunks_of(store.conn(), &e).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Un archivo sin aserción `availability` es un archivo del que no sabemos
/// nada. Tampoco se toca.
#[test]
fn sin_etiqueta_de_disponibilidad_no_hay_permiso() {
    let dir = temp_dir("sinetiqueta");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(&dir, "huerfano.md", "texto");

    let e = store.create_entity(1_000).unwrap();
    store
        .assert(NewAssertion::new(
            e.clone(),
            "path_observed_at",
            p.to_string_lossy().to_string(),
            Method::Deterministic,
            1_000,
        ))
        .unwrap();

    let r = correr(&mut store, 2_000);
    assert_eq!(r.skipped_not_local, 1);
    assert!(index::chunks_of(store.conn(), &e).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

// ═══════════════════════════════════════════════════ el criterio de "hecho"

/// El criterio de M2: buscar una palabra que **no está en el nombre del
/// archivo** y encontrar el archivo.
#[test]
fn busqueda_lexica_de_extremo_a_extremo() {
    let dir = temp_dir("busqueda");
    let mut store = Store::open_in_memory().unwrap();

    let a = escribir(
        &dir,
        "4b7878.txt",
        "Certificado de matrícula del curso 2025-2026.\n\n\
         Presentado ante el consulado durante el trámite de visado.",
    );
    let b = escribir(&dir, "otro.md", "Una nota sobre jardinería y macetas.");
    ingerir(&store, &a, "local", 1_000);
    ingerir(&store, &b, "local", 1_000);
    correr(&mut store, 2_000);

    let tp = TimePoint::at(3_000);
    let hits = search(&store, "matricula", 10, tp).unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].path.ends_with("4b7878.txt"));
    assert!(
        hits[0].snippet.contains('«'),
        "el snippet debe marcar el termino: {:?}",
        hits[0].snippet
    );

    // Sin diacríticos: el archivo dice "matrícula", la consulta dice
    // "matricula". Sin `remove_diacritics 2` esto no encontraría nada.
    assert!(!search(&store, "TRÁMITE", 10, tp).unwrap().is_empty());
    // Y la puntuación BM25 existe y ordena (más negativa = mejor).
    assert!(hits[0].score < 0.0, "bm25 = {}", hits[0].score);
    // Un término que no está en ningún sitio no inventa resultados (I9).
    assert!(search(&store, "hipopotamo", 10, tp).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn la_prueba_parcial_muestra_las_palabras_que_recuperaron_el_documento() {
    struct Limpiar(PathBuf);
    impl Drop for Limpiar {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = temp_dir("prueba-parcial");
    let _limpiar = Limpiar(dir.clone());
    let mut store = Store::open_in_memory().unwrap();
    let texto = format!(
        "Las notas con fecha antigua. {}La factura corresponde al contrato vigente.",
        "La documentación contiene descripciones administrativas. ".repeat(8)
    );
    let path = escribir(&dir, "nota.txt", &texto);
    let entity = ingerir(&store, &path, "local", 1_000);
    correr(&mut store, 2_000);

    let hits = search(
        &store,
        "Las facturas con contratos olvidados",
        10,
        TimePoint::at(3_000),
    )
    .unwrap();
    assert_eq!(
        hits.len(),
        1,
        "la coincidencia parcial debe recuperar el archivo"
    );
    let snippet = &hits[0].snippet;
    let chunks = index::chunks_of(store.conn(), &entity).unwrap();
    assert!(
        chunks.iter().any(|(_, text)| text.contains(snippet)),
        "la prueba debe ser literal"
    );
    assert!(
        snippet.contains("factura"),
        "prueba sin factura: {snippet:?}"
    );
    assert!(
        snippet.contains("contrato"),
        "prueba sin contrato: {snippet:?}"
    );
}

#[test]
fn la_busqueda_devuelve_un_resultado_por_archivo() {
    let dir = temp_dir("dedupe");
    let mut store = Store::open_in_memory().unwrap();
    // El mismo término veinte veces, en párrafos distintos: muchos fragmentos.
    let cuerpo = (0..20)
        .map(|i| format!("Parrafo {i} sobre el expediente administrativo."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let p = escribir(&dir, "largo.md", &cuerpo);
    ingerir(&store, &p, "local", 1_000);
    correr(&mut store, 2_000);

    let hits = search(&store, "expediente", 10, TimePoint::at(3_000)).unwrap();
    assert_eq!(hits.len(), 1, "un archivo no puede ocupar diez puestos");

    let _ = std::fs::remove_dir_all(&dir);
}

// ═══════════════════════════════════════════════════════ reejecutar y editar

#[test]
fn reejecutar_no_reextrae_ni_infla_el_log() {
    let dir = temp_dir("idempotente");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(&dir, "notas.md", "Contenido estable que no cambia.");
    ingerir(&store, &p, "local", 1_000);

    let r1 = correr(&mut store, 2_000);
    assert_eq!(r1.extracted, 1);
    let log1 = store.log_len().unwrap();
    let chunks1 = index::chunk_count(store.conn()).unwrap();

    let r2 = correr(&mut store, 3_000);
    assert_eq!(r2.unchanged, 1, "el hash del archivo no cambio");
    assert_eq!(r2.opened, 0, "no hay que volver a abrirlo siquiera");
    assert_eq!(store.log_len().unwrap(), log1, "el log no debe crecer");
    assert_eq!(index::chunk_count(store.conn()).unwrap(), chunks1);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Editar un archivo **sustituye** sus fragmentos. Que el log sea append-only
/// no significa que el índice acumule versiones del texto.
#[test]
fn editar_un_archivo_sustituye_sus_fragmentos() {
    let dir = temp_dir("editado");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(
        &dir,
        "notas.md",
        "Version primera con la palabra alcachofa.",
    );
    let e = ingerir(&store, &p, "local", 1_000);
    correr(&mut store, 2_000);
    assert_eq!(
        search(&store, "alcachofa", 5, TimePoint::at(2_500))
            .unwrap()
            .len(),
        1
    );

    // El archivo cambia; M1 volvería a escribir `has_hash`.
    escribir(
        &dir,
        "notas.md",
        "Version segunda con la palabra berenjena.",
    );
    let bytes = std::fs::read(&p).unwrap();
    store
        .assert(NewAssertion::new(
            e.clone(),
            "has_hash",
            blake3::hash(&bytes).to_hex().to_string(),
            Method::Deterministic,
            3_000,
        ))
        .unwrap();

    let r = correr(&mut store, 4_000);
    assert_eq!(r.extracted, 1, "el hash cambio: hay que reextraer");

    let tp = TimePoint::at(5_000);
    assert!(
        search(&store, "alcachofa", 5, tp).unwrap().is_empty(),
        "el texto viejo sigue en el indice"
    );
    assert_eq!(search(&store, "berenjena", 5, tp).unwrap().len(), 1);
    assert_eq!(index::chunks_of(store.conn(), &e).unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ═══════════════════════════════════════════════════════════ log y proyección

#[test]
fn el_log_registra_el_hecho_de_extraer_no_el_texto() {
    let dir = temp_dir("log");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(
        &dir,
        "acta.md",
        "Acta de la sesion de trabajo del 14 de enero.",
    );
    let e = ingerir(&store, &p, "local", 1_000);
    correr(&mut store, 2_000);

    let tp = TimePoint::at(3_000);
    let status = store.resolve(&e, "extraction_status", tp).unwrap().unwrap();
    assert_eq!(status.object.as_text(), Some(Status::Ok.as_str()));
    assert_eq!(status.method, Method::Deterministic);
    assert_eq!(status.source, "extract@m2");

    // I7: el chunker queda anotado en el log, no sólo en la tabla.
    let chunker = store.resolve(&e, "chunker", tp).unwrap().unwrap();
    assert_eq!(chunker.object.as_text(), Some(chunk::chunker_id().as_str()));

    // La faceta que convierte a la entidad en candidata a embeddings en M4.
    assert!(store.facets(&e, tp).unwrap().iter().any(|f| f == "textual"));

    // Y el texto NO está en el log: es proyección, no verdad (I1).
    let en_el_log = store
        .query(Some(&e), None, tp)
        .unwrap()
        .iter()
        .filter_map(|a| a.object.as_text().map(str::to_string))
        .any(|t| t.contains("sesion de trabajo"));
    assert!(!en_el_log, "el texto extraido no debe vivir en el log");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Reconstruir el índice desde cero no toca el log: es lo que hace que I1 sea
/// verdad y no un eslogan.
#[test]
fn el_indice_se_puede_tirar_y_rehacer() {
    let dir = temp_dir("rebuild");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(&dir, "acta.md", "Reunion sobre el flujograma comercial.");
    ingerir(&store, &p, "local", 1_000);
    correr(&mut store, 2_000);

    let log_antes = store.log_len().unwrap();
    let tp = TimePoint::at(3_000);
    assert_eq!(search(&store, "flujograma", 5, tp).unwrap().len(), 1);

    store
        .conn()
        .execute_batch("DELETE FROM chunks; DELETE FROM extractions;")
        .unwrap();
    assert!(search(&store, "flujograma", 5, tp).unwrap().is_empty());

    let r = correr(&mut store, 4_000);
    assert_eq!(r.extracted, 1);
    assert_eq!(
        search(&store, "flujograma", 5, TimePoint::at(5_000))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store.log_len().unwrap(),
        log_antes,
        "rehacer una proyeccion no escribe en el log"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ══════════════════════════════════════════════════════════ basura del corpus

/// Un `.log` de LevelDB de Chromium: extensión de texto, contenido binario.
/// De estos hay diez en el corpus real, dentro de `Rockstar Games`.
#[test]
fn un_binario_disfrazado_de_texto_no_entra_en_el_indice() {
    let dir = temp_dir("binario");
    let mut store = Store::open_in_memory().unwrap();

    let p = dir.join("000003.log");
    let mut bytes = b"\x01\x00\x00\x00leveldb.BytewiseComparator".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    std::fs::write(&p, &bytes).unwrap();
    let e = ingerir(&store, &p, "local", 1_000);

    let r = correr(&mut store, 2_000);
    assert_eq!(r.binary, 1);
    assert_eq!(r.extracted, 0);
    assert!(index::chunks_of(store.conn(), &e).unwrap().is_empty());

    // Pero **sí** queda anotado en el log: que un `.log` sea binario no se
    // deduce de la ruta, sólo de haberlo abierto.
    let s = store
        .resolve(&e, "extraction_status", TimePoint::at(3_000))
        .unwrap()
        .unwrap();
    assert_eq!(s.object.as_text(), Some("binary"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Los formatos que no sabemos leer no ensucian el log con una aserción por
/// archivo: se deducen de la ruta y ya están implícitos.
#[test]
fn lo_no_soportado_no_escribe_en_el_log() {
    let dir = temp_dir("nosoportado");
    let mut store = Store::open_in_memory().unwrap();
    let p = escribir(&dir, "captura.png", "no importa el contenido");
    let e = ingerir(&store, &p, "local", 1_000);

    let antes = store.log_len().unwrap();
    let r = correr(&mut store, 2_000);
    assert_eq!(r.skipped_unsupported, 1);
    assert_eq!(store.log_len().unwrap(), antes, "no se escribio nada");
    assert!(
        store
            .resolve(&e, "extraction_status", TimePoint::at(3_000))
            .unwrap()
            .is_none()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ═════════════════════════════════════════ recomponer el documento para verlo

/// El texto que se enseña en el panel de M5b sale de aquí, y si los fragmentos
/// se concatenaran a pelo el usuario leería cada párrafo dos veces. Con 200
/// bytes de solape y 11,6 fragmentos por archivo de media, no es un caso raro:
/// es todos los archivos.
#[test]
fn el_documento_se_recompone_sin_repetir_el_solape() {
    let dir = temp_dir("recomponer");
    let mut store = Store::open_in_memory().unwrap();

    // Lo bastante largo para que el chunker parta de verdad y haya solape.
    let parrafos: Vec<String> = (0..40)
        .map(|i| format!("Parrafo numero {i} con suficiente texto como para que el empaquetador tenga que cerrar el fragmento antes de tiempo y dejar el solape puesto."))
        .collect();
    let original = parrafos.join("\n\n");
    let p = escribir(&dir, "largo.txt", &original);
    let e = ingerir(&store, &p, "local", 1_000);
    correr(&mut store, 2_000);

    let trozos = index::chunks_of(store.conn(), &e).unwrap();
    assert!(
        trozos.len() > 1,
        "haria falta mas de un fragmento: {}",
        trozos.len()
    );

    let doc = index::document_text(store.conn(), &e)
        .unwrap()
        .expect("hay texto");
    let pegados: usize = trozos.iter().map(|(_, t)| t.len()).sum();
    assert!(
        doc.len() < pegados,
        "no se quito el solape: {} vs {pegados}",
        doc.len()
    );

    // La propiedad que importa: cada parrafo aparece **una vez**.
    for (i, parrafo) in parrafos.iter().enumerate() {
        assert_eq!(
            doc.matches(parrafo.as_str()).count(),
            1,
            "el parrafo {i} sale repetido"
        );
    }
    assert_eq!(doc, obrain_extract::text::normalize(&original).trim_end());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Una entidad sin fragmentos no devuelve una cadena vacia, que se leeria como
/// «este archivo esta vacio». Devuelve `None`, y el panel dice por que.
#[test]
fn sin_fragmentos_no_hay_documento() {
    let store = Store::open_in_memory().unwrap();
    index::migrate(store.conn()).unwrap();
    let e = store.create_entity(1_000).unwrap();
    assert_eq!(index::document_text(store.conn(), &e).unwrap(), None);
}
