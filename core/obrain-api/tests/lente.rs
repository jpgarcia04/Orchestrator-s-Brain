//! Los tests de M5a atacan **la lente**, no el dibujo.
//!
//! El criterio del hito no es que se vea bonito: es que lo que se enseña sea
//! una lente honesta sobre el grafo (I2) y no un volcado recortado. Un mapa que
//! esconde que está enseñando una parte, o que coloca los nodos en un sitio
//! distinto cada vez, ha fallado aunque renderice a 60 fps.

use obrain_api::{lens, Brain};
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint, Value};

const T: i64 = 1_000;

fn at() -> TimePoint {
    TimePoint::at(T + 10_000)
}

/// El presente de verdad.
///
/// Casi todo aquí vive en una época ficticia (`T = 1000`) porque es más legible,
/// pero [`Brain::correct`] sella con el reloj de pared —que es lo correcto: una
/// corrección ocurre cuando el usuario la hace—. Consultarla desde la época
/// ficticia no la encuentra, y no es un fallo: es I6 haciendo exactamente lo que
/// promete, que `asserted_at <= known` decide qué se sabía en cada momento.
fn ahora() -> TimePoint {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    TimePoint::at(ms)
}

fn base() -> Store {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    store
}

/// Un archivo: entidad con ruta, que es lo que le da nombre legible.
fn archivo(store: &Store, path: &str) -> EntityId {
    let e = store.create_entity(T).unwrap();
    store
        .assert(NewAssertion::new(
            e.clone(),
            "path_observed_at",
            path,
            Method::Deterministic,
            T,
        ))
        .unwrap();
    e
}

fn arista(store: &Store, from: &EntityId, pred: &str, to: &EntityId, m: Method, conf: f64) {
    store
        .assert(
            NewAssertion::new(from.clone(), pred, Value::Entity(to.clone()), m, T)
                .confidence(conf)
                .source("test"),
        )
        .unwrap();
}

/// Una carpeta con `n` archivos dentro, como la deja M3: el hub es el **objeto**
/// de todas sus aristas, nunca el sujeto.
fn carpeta(store: &Store, nombre: &str, n: usize) -> (EntityId, Vec<EntityId>) {
    let hub = store.create_entity(T).unwrap();
    store
        .assert(NewAssertion::new(
            hub.clone(),
            "folder_path",
            format!("C:\\Docs\\{nombre}"),
            Method::Deterministic,
            T,
        ))
        .unwrap();
    let hijos: Vec<EntityId> = (0..n)
        .map(|i| {
            let f = archivo(store, &format!("C:\\Docs\\{nombre}\\doc{i}.pdf"));
            arista(store, &f, "contained_by", &hub, Method::Deterministic, 1.0);
            f
        })
        .collect();
    (hub, hijos)
}

// ───────────────────────────────────────────────────────────────── la lente

/// **El motivo por el que existe `Store::query_by_object`.**
///
/// Un nodo concentrador de M3 —una carpeta, una sesión— no es sujeto de ninguna
/// arista: son sus 40 hijos los que apuntan hacia él. Una travesía que sólo
/// mirase hacia delante lo dejaría aislado justo a él, que es el nodo que más
/// significa del grafo entero.
#[test]
fn el_hub_aparece_aunque_solo_sea_objeto_de_sus_aristas() {
    let store = base();
    let (hub, hijos) = carpeta(&store, "Tramites", 5);

    let l = lens::open(&store, &[hijos[0].clone()], 1, 50, at()).unwrap();

    assert!(
        l.nodes.iter().any(|n| n.id == hub.0),
        "la carpeta tiene que estar: es el vecino del archivo"
    );
    assert_eq!(l.nodes.len(), 2, "radio 1 desde un archivo: el archivo y su carpeta");
}

/// Radio 2 desde un archivo llega a sus hermanos **a través** del hub, que es
/// exactamente la topología que M3 buscaba con «hubs, no cliques».
#[test]
fn radio_dos_alcanza_a_los_hermanos_por_el_hub() {
    let store = base();
    let (_, hijos) = carpeta(&store, "Tramites", 5);

    let l = lens::open(&store, &[hijos[0].clone()], 2, 50, at()).unwrap();

    assert_eq!(l.nodes.len(), 6, "los 5 archivos + la carpeta, y ninguno mas");
    assert!(!l.truncated);
    let hermano = l.nodes.iter().find(|n| n.id == hijos[3].0).unwrap();
    assert_eq!(hermano.distance, 2, "se llega a un hermano en dos saltos, no en uno");
}

/// I2: **presupuesto duro**. Y cuando corta, lo dice.
#[test]
fn el_presupuesto_corta_y_la_lente_lo_confiesa() {
    let store = base();
    let (_, hijos) = carpeta(&store, "Grande", 60);

    let l = lens::open(&store, &[hijos[0].clone()], 2, 10, at()).unwrap();

    assert_eq!(l.nodes.len(), 10, "el presupuesto es duro, no orientativo");
    assert!(l.truncated, "hay mas grafo del que se ensena y hay que decirlo");
    assert!(
        l.reached > l.nodes.len(),
        "y hay que decir cuanto: {} alcanzados frente a {} dibujados",
        l.reached,
        l.nodes.len()
    );
}

/// El corte es **por capas**: un vecino directo del foco nunca se queda fuera
/// para hacer sitio a uno de segundo grado. Es la diferencia con un `LIMIT`.
#[test]
fn el_corte_respeta_las_capas() {
    let store = base();
    let (hub, hijos) = carpeta(&store, "Grande", 60);

    let l = lens::open(&store, &[hijos[0].clone()], 2, 5, at()).unwrap();

    assert!(
        l.nodes.iter().any(|n| n.id == hub.0),
        "el vecino a distancia 1 entra antes que cualquiera a distancia 2"
    );
    let maxima = l.nodes.iter().map(|n| n.distance).max().unwrap();
    assert!(maxima <= 2);
}

/// **Prototipo #2 de `EXPERIENCE.md` §8, en el lado del servidor.**
///
/// Si el conjunto de nodos bailara entre dos ejecuciones de la misma consulta,
/// ninguna estabilidad de layout en el cliente podría arreglarlo: estaría
/// colocando cosas distintas.
#[test]
fn la_misma_consulta_devuelve_exactamente_el_mismo_subgrafo() {
    let store = base();
    let (_, hijos) = carpeta(&store, "Grande", 60);

    let a = lens::open(&store, &[hijos[0].clone()], 2, 25, at()).unwrap();
    let b = lens::open(&store, &[hijos[0].clone()], 2, 25, at()).unwrap();

    let ids = |l: &lens::Lens| l.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&a), ids(&b), "mismos nodos y en el mismo orden");
    assert_eq!(
        a.nodes.iter().map(|n| n.seed).collect::<Vec<_>>(),
        b.nodes.iter().map(|n| n.seed).collect::<Vec<_>>(),
        "y arrancando desde las mismas posiciones"
    );
}

/// La posición de partida se deriva del identificador, así que es la misma
/// dentro de tres años y sin guardar nada. Es la mitad barata de
/// `EXPERIENCE.md` §6.
#[test]
fn la_posicion_de_partida_no_depende_de_la_consulta() {
    let store = base();
    let (hub, hijos) = carpeta(&store, "Tramites", 8);

    let desde_archivo = lens::open(&store, &[hijos[0].clone()], 2, 50, at()).unwrap();
    let desde_hub = lens::open(&store, std::slice::from_ref(&hub), 1, 50, at()).unwrap();

    let buscar = |l: &lens::Lens, id: &str| l.nodes.iter().find(|n| n.id == id).unwrap().seed;
    assert_eq!(
        buscar(&desde_archivo, &hijos[3].0),
        buscar(&desde_hub, &hijos[3].0),
        "el mismo nodo arranca en el mismo sitio se llegue por donde se llegue"
    );
}

/// Una arista con un extremo fuera del presupuesto no se dibuja colgando hacia
/// la nada. Un cabo suelto se lee como un nodo que falta, y no falta: no cabía.
#[test]
fn ninguna_arista_apunta_fuera_de_lo_dibujado() {
    let store = base();
    let (_, hijos) = carpeta(&store, "Grande", 60);

    let l = lens::open(&store, &[hijos[0].clone()], 2, 12, at()).unwrap();

    let dentro: Vec<&str> = l.nodes.iter().map(|n| n.id.as_str()).collect();
    for e in &l.edges {
        assert!(dentro.contains(&e.from.as_str()), "arista desde un nodo que no se dibuja");
        assert!(dentro.contains(&e.to.as_str()), "arista hacia un nodo que no se dibuja");
    }
}

/// El grado es **global**, no el visible. Si dependiera de lo que cabe en
/// pantalla, el mismo nodo cambiaría de tamaño al cambiar de consulta y la
/// memoria espacial de `EXPERIENCE.md` §6 no funcionaría.
#[test]
fn el_grado_de_un_nodo_no_cambia_con_el_presupuesto() {
    let store = base();
    let (hub, hijos) = carpeta(&store, "Grande", 60);

    let amplia = lens::open(&store, &[hijos[0].clone()], 2, 300, at()).unwrap();
    let estrecha = lens::open(&store, &[hijos[0].clone()], 1, 3, at()).unwrap();

    let grado = |l: &lens::Lens| l.nodes.iter().find(|n| n.id == hub.0).unwrap().degree;
    assert_eq!(grado(&amplia), 60, "la carpeta tiene 60 aristas vivas");
    assert_eq!(grado(&estrecha), 60, "y sigue teniendo 60 aunque solo se dibujen 3 nodos");
}

/// Los canales visuales de `EXPERIENCE.md` §1 salen resueltos del núcleo: el
/// método decide el trazo y la confianza la opacidad. Una interfaz que los
/// dedujera por su cuenta sería una segunda definición de "esto es un hecho".
#[test]
fn cada_arista_llega_con_su_metodo_y_su_confianza() {
    let store = base();
    let a = archivo(&store, "C:\\x\\informe_v1.pdf");
    let b = archivo(&store, "C:\\x\\informe_v2.pdf");
    let c = archivo(&store, "C:\\x\\copia.pdf");
    arista(&store, &b, "version_of", &a, Method::Inferred, 0.75);
    arista(&store, &c, "duplicate_of", &a, Method::Deterministic, 1.0);

    let l = lens::open(&store, std::slice::from_ref(&a), 1, 50, at()).unwrap();

    let version = l.edges.iter().find(|e| e.predicate == "version_of").unwrap();
    assert_eq!(version.method, "inferred", "punteada: es una heuristica");
    assert!((version.confidence - 0.75).abs() < 1e-9);
    assert_eq!(lens::stroke(Method::Inferred), "dashed");

    let dup = l.edges.iter().find(|e| e.predicate == "duplicate_of").unwrap();
    assert_eq!(dup.method, "deterministic", "solida: es un hash");
    assert_eq!(lens::stroke(Method::Deterministic), "solid");
}

/// La bóveda **sí** se dibuja, y llega marcada.
///
/// No es una fuga: §5.2 le concede a `vault` índice léxico local, y el mapa no
/// sale de la máquina. Lo que no puede pasar es que se dibuje igual que lo
/// demás, porque entonces el usuario no sabe qué está mirando. La marca viaja
/// con el nodo para que la interfaz no tenga que preguntar.
#[test]
fn un_nodo_de_boveda_llega_marcado() {
    let store = base();
    let (_, hijos) = carpeta(&store, "Tramites", 3);
    store.set_sensitivity(&hijos[1], Sensitivity::Vault).unwrap();

    let l = lens::open(&store, &[hijos[0].clone()], 2, 50, at()).unwrap();

    let secreto = l.nodes.iter().find(|n| n.id == hijos[1].0).unwrap();
    assert_eq!(secreto.sensitivity, "vault");
    assert!(l.nodes.iter().filter(|n| n.sensitivity == "normal").count() >= 3);
}

/// Un foco sin ninguna arista no es un error: es un archivo suelto. El mapa
/// tiene que poder enseñar un solo punto.
#[test]
fn un_nodo_aislado_es_una_lente_de_un_nodo() {
    let store = base();
    let solo = archivo(&store, "C:\\suelto.txt");

    let l = lens::open(&store, std::slice::from_ref(&solo), 2, 50, at()).unwrap();

    assert_eq!(l.nodes.len(), 1);
    assert!(l.edges.is_empty());
    assert!(!l.truncated);
    assert_eq!(l.nodes[0].degree, 0);
}

// ─────────────────────────────────────────────────────── el panel de entidad

/// `EXPERIENCE.md` §4.4: el panel parte lo que se sabe en dos bloques, y el
/// corte es por método. Un hash y una sospecha de un clasificador no pueden
/// leerse igual (I9).
#[test]
fn el_panel_separa_el_hecho_de_la_sospecha() {
    let store = base();
    let a = archivo(&store, "C:\\x\\informe_v2.pdf");
    let b = archivo(&store, "C:\\x\\informe_v1.pdf");
    arista(&store, &a, "version_of", &b, Method::Inferred, 0.75);
    arista(&store, &a, "duplicate_of", &b, Method::Deterministic, 1.0);

    let brain = Brain::from_store(store);
    let v = brain.entity(&a).unwrap();

    assert!(v.observed.iter().any(|c| c.predicate == "duplicate_of"));
    assert!(v.inferred.iter().any(|c| c.predicate == "version_of"));
    assert!(
        !v.observed.iter().any(|c| c.predicate == "version_of"),
        "una heuristica no puede colarse en el bloque solido"
    );
    // El vocabulario controlado trae su etiqueta: `version_of` → "version de".
    let ver = v.inferred.iter().find(|c| c.predicate == "version_of").unwrap();
    assert_eq!(ver.label, "version de");
    assert!(ver.value_entity.is_some(), "el objeto es una entidad y se puede navegar");
}

/// I5 en la superficie: la corrección del usuario gana, aparece en el bloque
/// sólido, y **la inferencia que corrige sigue ahí**. Borrar el error borraría
/// la prueba de que la corrección sirvió de algo.
#[test]
fn una_correccion_gana_y_no_borra_lo_que_corrige() {
    let store = base();
    let a = archivo(&store, "C:\\x\\informe_v2.pdf");
    let b = archivo(&store, "C:\\x\\informe_v1.pdf");
    let c = archivo(&store, "C:\\x\\otro.pdf");
    arista(&store, &a, "version_of", &b, Method::Inferred, 0.75);

    let brain = Brain::from_store(store);
    brain
        .correct(&a, "version_of", Value::Entity(c.clone()))
        .unwrap();

    let v = brain.entity(&a).unwrap();
    let correccion = v
        .observed
        .iter()
        .find(|x| x.predicate == "version_of")
        .expect("la correccion es un hecho y va al bloque solido");
    assert!(correccion.by_user);
    assert_eq!(correccion.value_entity.as_deref(), Some(c.0.as_str()));

    assert!(
        v.inferred.iter().any(|x| x.predicate == "version_of"),
        "la inferencia equivocada sigue en el log y sigue visible"
    );

    // Y quien resuelva el valor único ve la del usuario, no la otra.
    let ganadora = brain
        .store()
        .resolve(&a, "version_of", ahora())
        .unwrap()
        .unwrap();
    assert_eq!(ganadora.method, Method::User);
    assert_eq!(ganadora.object, Value::Entity(c));
}

/// La contabilidad interna del pipeline no es información sobre el archivo.
/// Enseñarla llenaría de ruido justo el sitio donde hay que confiar de un
/// vistazo.
#[test]
fn el_panel_no_ensena_la_fontaneria() {
    let store = base();
    let a = archivo(&store, "C:\\x\\doc.pdf");
    store
        .assert(NewAssertion::new(a.clone(), "chunk_count", 12.0, Method::Deterministic, T))
        .unwrap();
    store
        .assert(NewAssertion::new(a.clone(), "has_facet", "textual", Method::Deterministic, T))
        .unwrap();

    let brain = Brain::from_store(store);
    let v = brain.entity(&a).unwrap();

    assert!(!v.observed.iter().any(|c| c.predicate == "chunk_count"));
    assert!(!v.observed.iter().any(|c| c.predicate == "has_facet"));
    assert!(!v.observed.iter().any(|c| c.predicate == "path_observed_at"));
    // ...pero la faceta sí llega, en su sitio: es "qué es", no una fila más.
    assert_eq!(v.facets, vec!["textual"]);
    assert_eq!(v.path.as_deref(), Some("C:\\x\\doc.pdf"));
}

#[test]
fn una_entidad_que_no_existe_es_un_error_y_no_una_vista_vacia() {
    let brain = Brain::from_store(base());
    assert!(brain.entity(&EntityId::from("NO_EXISTE")).is_err());
}

// ─────────────────────────────────────────────────────────────── la búsqueda

/// **La degradación es la característica, no el plan B.**
///
/// Cargar `multilingual-e5-small` cuesta 2,50 s. Una barra que no responde
/// hasta que termine ha perdido el argumento de latencia de `EXPERIENCE.md` §2
/// antes de enseñar nada. Sin modelo se busca en léxico y se dice que la mitad
/// semántica no está.
#[test]
fn sin_modelo_la_barra_responde_igual_y_lo_admite() {
    let store = base();
    let a = archivo(&store, "C:\\x\\matricula.pdf");
    obrain_extract::index::replace_chunks(
        store.conn(),
        &a,
        "paragraph-pack",
        1,
        &[obrain_extract::chunk::Chunk {
            ordinal: 0,
            byte_start: 0,
            byte_end: 40,
            text: "Titulacion y Centro Docente Privado".into(),
        }],
    )
    .unwrap();

    let brain = Brain::from_store(store);
    assert!(!brain.semantic_ready());

    let r = brain.search("Titulacion", 10).unwrap();
    assert_eq!(r.hits.len(), 1);
    assert_eq!(r.branches, vec!["bm25"]);
    assert!(!r.semantic, "hay que decir que la mitad semantica no ha corrido");
    assert_eq!(r.hits[0].ranks, vec![("bm25".to_string(), 1)]);
    assert_eq!(r.hits[0].label, "matricula.pdf", "el nombre lo decide M3, no la interfaz");
    assert_eq!(r.hits[0].sensitivity, "normal");
}

/// El sustrato de `ADR-0004`: mientras se escribe, el mapa del fondo se
/// enciende por donde cae la respuesta. Es la misma lente, con los resultados
/// como foco.
#[test]
fn el_sustrato_se_abre_sobre_los_resultados_de_la_consulta() {
    let store = base();
    let (hub, hijos) = carpeta(&store, "Tramites", 4);
    obrain_extract::index::replace_chunks(
        store.conn(),
        &hijos[0],
        "paragraph-pack",
        1,
        &[obrain_extract::chunk::Chunk {
            ordinal: 0,
            byte_start: 0,
            byte_end: 20,
            text: "expediente de visado".into(),
        }],
    )
    .unwrap();

    let brain = Brain::from_store(store);
    let g = brain.glance("expediente", 10).unwrap();

    assert_eq!(g.answer.hits.len(), 1, "la lista y el sustrato salen de una sola busqueda");
    assert_eq!(g.lens.focus, vec![hijos[0].0.clone()]);
    assert!(g.lens.nodes.iter().any(|n| n.id == hub.0), "y su carpeta se enciende con el");
    assert_eq!(
        g.lens.nodes.iter().find(|n| n.id == hijos[0].0).unwrap().distance,
        0
    );
}

/// Una consulta que no encuentra nada abre una lente vacía, no una lente rota.
#[test]
fn una_consulta_sin_resultados_no_dibuja_nada() {
    let brain = Brain::from_store(base());
    let g = brain.glance("nada de esto existe", 10).unwrap();
    assert!(g.answer.hits.is_empty());
    assert!(g.lens.nodes.is_empty());
    assert!(g.lens.edges.is_empty());
    assert!(!g.lens.truncated);
}

// ──────────────────────────────────────────────────────────── el censo previo

/// `--sample` para el mapa: qué va a salir grande **antes** de dibujarlo.
#[test]
fn el_censo_ensena_quien_va_a_salir_grande() {
    let store = base();
    carpeta(&store, "Grande", 30);
    carpeta(&store, "Pequena", 2);

    let censo = lens::edge_census(&store, at()).unwrap();
    assert_eq!(censo[0], ("contained_by".to_string(), 32));

    // Y con su padre: cinco carpetas llamadas `src` serían cinco nodos
    // indistinguibles en el mapa.
    let hubs = lens::hubs(&store, 3, at()).unwrap();
    assert_eq!(hubs[0], ("Docs/Grande".to_string(), 30));
    assert_eq!(hubs[1], ("Docs/Pequena".to_string(), 2));
}
