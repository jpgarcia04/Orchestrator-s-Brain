//! El explorador de [ADR-0014]: bajar por el árbol sin escribir nada.
//!
//! Igual que en `cielo.rs`, el árbol lo monta el **enlazador real de M3** y no
//! una mano: lo que se prueba es que se puede bajar por lo que el sistema
//! escribe de verdad.
//!
//! Lo que estos tests protegen no es una función, es la consecuencia #4 del ADR
//! —*«dos vistas de un solo estado»*—. En el momento en que el explorador y la
//! galaxia calculen por su cuenta cuántos archivos hay dentro de una carpeta,
//! dejan de ser dos vistas y pasan a ser dos aplicaciones compartiendo ventana.
//!
//! [ADR-0014]: ../../../docs/adr/0014-dos-vistas-un-solo-estado.md

use obrain_api::sky;
use obrain_core::{EntityId, Method, NewAssertion, Store, TimePoint, Value};
use obrain_link::{FileView, LinkReport};

const T: i64 = 1_700_000_000_000;

fn at() -> TimePoint {
    TimePoint::at(T + 10_000)
}

fn base() -> Store {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    store
}

fn arbol(store: &Store, paths: &[&str]) -> Vec<EntityId> {
    let files: Vec<FileView> = paths
        .iter()
        .map(|p| {
            let e = store.create_entity(T).unwrap();
            store
                .assert(NewAssertion::new(
                    e.clone(),
                    "path_observed_at",
                    *p,
                    Method::Deterministic,
                    T,
                ))
                .unwrap();
            FileView {
                entity: e,
                path: (*p).to_string(),
                hash: None,
                modified_ms: T,
                size: 1,
                availability: "local".into(),
            }
        })
        .collect();
    let mut r = LinkReport::default();
    obrain_link::structure::link(store, &files, T, &mut r).unwrap();
    files.into_iter().map(|f| f.entity).collect()
}

/// La carpeta que se llama así, en la lista que sea.
///
/// Compara **la hoja de la ruta** y no la etiqueta: la etiqueta puede llevar
/// un aclarado entre paréntesis cuando dos hermanas se llaman igual.
fn carpeta(l: &sky::Listing, nombre: &str) -> EntityId {
    l.rows
        .iter()
        .find(|r| r.kind == "folder" && r.path.rsplit(['\\', '/']).next() == Some(nombre))
        .map(|r| EntityId(r.id.clone()))
        .unwrap_or_else(|| panic!("no hay carpeta «{nombre}» en {:?}", etiquetas(l)))
}

fn etiquetas(l: &sky::Listing) -> Vec<&str> {
    l.rows.iter().map(|r| r.label.as_str()).collect()
}

/// La única raíz del árbol.
///
/// `obrain-link` **no crea un nodo para la unidad**: la raíz es la carpeta más
/// alta que contiene algo, así que con todo bajo `C:\vida` la raíz es `vida`
/// y no `C:`. Y con una sola raíz el explorador **entra en ella** al abrirse:
/// pedir las raíces devuelve ya su contenido, con el foco puesto en ella.
fn raiz(store: &Store) -> EntityId {
    let l = sky::children(store, None, at()).unwrap();
    let foco = l.focus.clone();
    EntityId(
        foco.unwrap_or_else(|| panic!("este fixture tiene una sola raíz: {:?}", etiquetas(&l))),
    )
}

/// Baja hasta el fondo contando archivos. Es la cuenta que el explorador tiene
/// que poder sostener: lo que promete una fila es lo que hay al entrar en ella.
fn contando_hacia_abajo(store: &Store, id: &EntityId) -> usize {
    let l = sky::children(store, Some(id), at()).unwrap();
    let mut n = l.files;
    for r in l.rows.iter().filter(|r| r.kind == "folder") {
        n += contando_hacia_abajo(store, &EntityId(r.id.clone()));
    }
    n
}

// ─────────────────────────────────────────────────────────────── bajar un nivel

/// Sin foco se entra por las raíces. Es el estado inicial del explorador cuando
/// nadie ha enfocado nada todavía en la galaxia.
#[test]
fn sin_foco_se_entra_por_las_raices() {
    let store = base();
    arbol(&store, &["C:\\vida\\a.txt", "D:\\copias\\b.txt"]);

    let raiz = sky::children(&store, None, at()).unwrap();
    assert_eq!(raiz.focus, None);
    assert_eq!(
        raiz.folders,
        2,
        "dos volúmenes, dos raíces: {:?}",
        etiquetas(&raiz)
    );
    assert_eq!(raiz.files, 0, "de una raíz no cuelga ningún archivo suelto");
    assert!(raiz.trail.is_empty(), "en la raíz no hay migas que enseñar");
}

/// Las carpetas van delante de los archivos, y por peso. Quien está bajando
/// busca casi siempre la carpeta, no el archivo suelto de este nivel.
#[test]
fn las_carpetas_van_primero_y_por_peso() {
    let store = base();
    arbol(
        &store,
        &[
            "C:\\vida\\suelto.txt",
            "C:\\vida\\pocas\\a.txt",
            "C:\\vida\\muchas\\a.txt",
            "C:\\vida\\muchas\\b.txt",
            "C:\\vida\\muchas\\c.txt",
        ],
    );

    let l = sky::children(&store, Some(&raiz(&store)), at()).unwrap();

    assert_eq!(l.folders, 2);
    assert_eq!(l.files, 1);
    assert_eq!(l.rows[0].kind, "folder");
    assert!(
        l.rows[0].label.contains("muchas"),
        "la mayor primero: {:?}",
        etiquetas(&l)
    );
    assert!(l.rows[1].label.contains("pocas"));
    assert_eq!(
        l.rows[2].kind, "file",
        "los archivos, detrás de las carpetas"
    );
    assert_eq!(
        l.rows[0].weight, 3,
        "el peso es lo que hay dentro de verdad"
    );
}

// ───────────────────────────────────── la consecuencia #4: un solo estado

/// **Lo que promete una fila es lo que hay al entrar.** Si el peso que enseña
/// una carpeta no es el número de archivos que se alcanzan bajando por ella, la
/// galaxia dice «229» y el explorador enseña 217 — y ahí se rompe el ADR.
#[test]
fn el_peso_de_una_fila_es_lo_que_se_encuentra_al_bajar() {
    let store = base();
    arbol(
        &store,
        &[
            "C:\\vida\\viajes\\roma\\1.jpg",
            "C:\\vida\\viajes\\roma\\2.jpg",
            "C:\\vida\\viajes\\barcelona\\3.jpg",
            "C:\\vida\\viajes\\suelta.jpg",
            "C:\\vida\\trabajo\\informe.pdf",
        ],
    );

    let l = sky::children(&store, Some(&raiz(&store)), at()).unwrap();
    for fila in l.rows.iter().filter(|r| r.kind == "folder") {
        let id = EntityId(fila.id.clone());
        assert_eq!(
            fila.weight,
            contando_hacia_abajo(&store, &id),
            "«{}» promete {} archivos y al bajar hay otra cosa",
            fila.label,
            fila.weight,
        );
    }
}

/// Las migas llevan de vuelta hasta la raíz, en orden y sin saltarse un nivel.
/// Son la mitad de la continuidad de estado que el ADR exige: sin ellas, entrar
/// al explorador desde la galaxia te deja sin saber dónde estás.
#[test]
fn las_migas_reconstruyen_el_camino_entero() {
    let store = base();
    arbol(&store, &["C:\\vida\\viajes\\roma\\foto.jpg"]);

    let vida = raiz(&store);
    let viajes = carpeta(&sky::children(&store, Some(&vida), at()).unwrap(), "viajes");
    let roma = carpeta(&sky::children(&store, Some(&viajes), at()).unwrap(), "roma");

    let l = sky::children(&store, Some(&roma), at()).unwrap();
    let camino: Vec<&str> = l.trail.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        camino.len(),
        2,
        "vida → viajes, de fuera hacia dentro: {camino:?}"
    );
    assert!(
        camino[0].ends_with("vida"),
        "la de fuera va primero: {camino:?}"
    );
    assert!(camino[1].ends_with("viajes"));
    assert_eq!(l.files, 1, "y dentro, la foto");
}

// ──────────────────────────────────── navegar el grafo, no el disco

/// **La diferencia que sostiene el proyecto.** Una sesión de trabajo no es una
/// carpeta y aun así se baja por ella: agrupa por *cuándo* tocaste las cosas, y
/// eso un explorador de archivos no lo puede enseñar.
#[test]
fn se_baja_por_una_sesion_igual_que_por_una_carpeta() {
    let store = base();
    let files = arbol(
        &store,
        &[
            "C:\\vida\\trabajo\\informe.pdf",
            "C:\\vida\\viajes\\billete.pdf",
        ],
    );

    // Una sesión que cruzó dos carpetas: exactamente el caso que el árbol de
    // contención no sabe contar.
    let sesion = store.create_entity(T).unwrap();
    for f in &files {
        store
            .assert(NewAssertion::new(
                f.clone(),
                "occurred_during",
                Value::Entity(sesion.clone()),
                Method::Deterministic,
                T,
            ))
            .unwrap();
    }

    let l = sky::children(&store, Some(&sesion), at()).unwrap();
    assert_eq!(
        l.files, 2,
        "los dos archivos de la sesión, de dos carpetas distintas"
    );
    assert_eq!(l.folders, 0, "una sesión no tiene subcarpetas");
    assert!(l.rows.iter().all(|r| r.kind == "file"));
}

// ─────────────────────────────────────────── la guarda de la previsualización

/// `availability` viaja en cada fila. Es la señal que impide que una
/// previsualización descargue un archivo de la nube sin querer, y por eso la
/// trae el núcleo ya resuelta en vez de dejar que la interfaz la deduzca.
#[test]
fn cada_fila_dice_si_su_contenido_se_puede_leer() {
    let store = base();
    let files = arbol(&store, &["C:\\vida\\local.txt", "C:\\vida\\nube.txt"]);
    for (f, v) in files.iter().zip(["local", "cloud_dehydrated"]) {
        store
            .assert(NewAssertion::new(
                f.clone(),
                "availability",
                v,
                Method::Deterministic,
                T,
            ))
            .unwrap();
    }

    let l = sky::children(&store, Some(&raiz(&store)), at()).unwrap();

    let nube = l.rows.iter().find(|r| r.label.contains("nube")).unwrap();
    assert_eq!(nube.availability.as_deref(), Some("cloud_dehydrated"));
    assert!(
        !obrain_scan::triage::Availability::parse(nube.availability.as_deref().unwrap())
            .unwrap()
            .may_read_content(),
        "leer este archivo lo descargaría: la fila tiene que poder decirlo"
    );

    let local = l.rows.iter().find(|r| r.label.contains("local")).unwrap();
    assert_eq!(local.availability.as_deref(), Some("local"));
}

// ──────────────────────────────────────────────────────────────── bibliotecas

/// La carpeta que el usuario eligió indexar, marcada como la marcaría el escáner.
fn biblioteca(store: &Store, ruta: &str) -> EntityId {
    obrain_scan::register_library(store, std::path::Path::new(ruta), T).unwrap()
}

/// Con una sola biblioteca, abrir el explorador es abrir **tu bóveda**: sin
/// una lista de un único elemento delante, y sin el camino del disco encima.
#[test]
fn una_sola_biblioteca_se_abre_directamente() {
    let store = base();
    let lib = biblioteca(&store, "C:\\Users\\jp\\Bóveda");
    arbol(
        &store,
        &[
            "C:\\Users\\jp\\Bóveda\\pasaporte.pdf",
            "C:\\Users\\jp\\Bóveda\\Salud\\poliza.pdf",
        ],
    );

    let l = sky::children(&store, None, at()).unwrap();
    assert_eq!(
        l.focus.as_deref(),
        Some(lib.as_str()),
        "se entra en la biblioteca"
    );
    assert_eq!(
        l.label, "Bóveda",
        "se llama como la carpeta, sin el padre delante"
    );
    assert!(
        l.trail.is_empty(),
        "por encima de la bóveda no hay migas: {:?}",
        l.trail.len()
    );
    assert_eq!(etiquetas(&l), vec!["Salud", "pasaporte.pdf"]);
}

/// Una base escaneada **antes** de que existieran las bibliotecas tiene aristas
/// que suben hasta `C:\Users`. Marcar la carpeta como biblioteca tiene que
/// bastar para cortar ahí, sin reescribir el log.
#[test]
fn una_base_antigua_se_corta_en_la_biblioteca_sin_tocar_el_log() {
    let store = base();
    arbol(
        &store,
        &[
            "C:\\Users\\jp\\Bóveda\\pasaporte.pdf",
            "C:\\Users\\jp\\Bóveda\\Salud\\poliza.pdf",
        ],
    );
    // Sin biblioteca, la raíz es lo más alto que el enlazador encadenó.
    let antes = sky::children(&store, None, at()).unwrap();
    assert_eq!(
        antes.label,
        "Users",
        "el camino del disco: {:?}",
        etiquetas(&antes)
    );

    let lib = biblioteca(&store, "C:\\Users\\jp\\Bóveda");
    let salud = carpeta(&sky::children(&store, Some(&lib), at()).unwrap(), "Salud");
    let l = sky::children(&store, Some(&salud), at()).unwrap();
    let migas: Vec<&str> = l.trail.iter().map(|m| m.label.as_str()).collect();
    assert_eq!(migas, vec!["Bóveda"], "las migas empiezan en la bóveda");

    let raices = sky::children(&store, None, at()).unwrap();
    assert_eq!(
        raices.focus.as_deref(),
        Some(lib.as_str()),
        "el camino vacío no es una raíz"
    );
}

/// Dos bibliotecas que se llaman igual no pueden ser dos filas idénticas.
#[test]
fn dos_bibliotecas_con_el_mismo_nombre_se_distinguen() {
    let store = base();
    biblioteca(&store, "C:\\casa\\Documentos");
    biblioteca(&store, "D:\\trabajo\\Documentos");
    arbol(
        &store,
        &[
            "C:\\casa\\Documentos\\a.pdf",
            "D:\\trabajo\\Documentos\\b.pdf",
        ],
    );

    let l = sky::children(&store, None, at()).unwrap();
    let mut e = etiquetas(&l);
    e.sort();
    assert_eq!(e, vec!["Documentos (casa)", "Documentos (trabajo)"]);
}

// ─────────────────────────────────────────────────────── recientes y sensibles

/// Lo último que tocaste va primero, y comparte fila con el explorador: el
/// mismo nombre y el mismo peso que se ve al bajar por la carpeta.
#[test]
fn los_recientes_van_del_mas_nuevo_al_mas_viejo() {
    let store = base();
    let ids = arbol(
        &store,
        &[
            "C:\\vida\\viejo.txt",
            "C:\\vida\\nuevo.txt",
            "C:\\vida\\medio.txt",
        ],
    );
    for (e, t) in ids.iter().zip([T - 3_000, T - 1_000, T - 2_000]) {
        store
            .assert(NewAssertion::new(
                e.clone(),
                "modified_at",
                Value::Timestamp(t),
                Method::Deterministic,
                T,
            ))
            .unwrap();
    }
    let r = sky::recent(&store, 2, at()).unwrap();
    let nombres: Vec<&str> = r.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(
        nombres,
        vec!["nuevo.txt", "medio.txt"],
        "el tope se respeta y el orden es por fecha"
    );
    assert!(r.iter().all(|f| f.kind == "file"));
}

/// Sólo lo que la puerta retiene, y nada normal.
#[test]
fn los_sensibles_son_solo_lo_que_la_puerta_retiene() {
    let store = base();
    let ids = arbol(
        &store,
        &[
            "C:\\vida\\pasaporte.pdf",
            "C:\\vida\\receta.pdf",
            "C:\\vida\\clave.txt",
        ],
    );
    store
        .set_sensitivity(&ids[0], obrain_core::Sensitivity::Sensitive, 1)
        .unwrap();
    store
        .set_sensitivity(&ids[2], obrain_core::Sensitivity::Vault, 1)
        .unwrap();
    let s = sky::sensitive(&store, at()).unwrap();
    let nombres: Vec<&str> = s.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(nombres, vec!["clave.txt", "pasaporte.pdf"]);
}
