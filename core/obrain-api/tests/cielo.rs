//! El cielo: el grafo a poca escala (`EXPERIENCE.md` §6).
//!
//! Los fixtures montan el árbol con el **enlazador real de M3**, no a mano. Es
//! deliberado: lo que se prueba aquí es que la agregación funciona sobre lo que
//! el sistema escribe de verdad, y un árbol construido a mano puede tener una
//! forma que `obrain-link` nunca produce.

use obrain_api::{Brain, sky};
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint, Value};
use obrain_link::{FileView, LinkReport};

const T: i64 = 1_700_000_000_000;

fn at() -> TimePoint {
    TimePoint::at(T + 10_000)
}

/// Monta un árbol de carpetas real a partir de rutas.
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

fn base() -> Store {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    store
}

fn etiquetas(c: &sky::Sky) -> Vec<&str> {
    c.regions.iter().map(|r| r.label.as_str()).collect()
}

// ────────────────────────────────────────────────────────── partir por el mayor

/// Marca una carpeta ya creada con una faceta. En la aplicación lo hace M3 al
/// ver un `.git` en el disco; aquí se pone a mano porque el fixture inventa
/// rutas que no existen en ningún sitio.
fn faceta(store: &Store, ruta: &str, facet: &str) {
    let e = store
        .find_subject_by_value("folder_path", ruta, at())
        .unwrap()
        .unwrap();
    store
        .assert(NewAssertion::new(
            e,
            "has_facet",
            facet,
            Method::Deterministic,
            T,
        ))
        .unwrap();
}

/// Le da a un archivo un formato reconocido, que es lo que M2 escribe al triar.
fn con_formato(store: &Store, e: &EntityId) {
    store
        .assert(NewAssertion::new(
            e.clone(),
            "mime_guess",
            "application/pdf",
            Method::Deterministic,
            T,
        ))
        .unwrap();
}

// ─────────────────────────────────────────── lo que no se parte y lo que calla

/// **Un proyecto de código es una sola estrella, y un taller de proyectos
/// también.**
///
/// Medido sobre el corpus real antes de existir esta regla: los siete
/// repositorios de `Documentos\GitHub` se llevaban **16 de las 30 regiones**, y
/// la carpeta donde vive el trámite de la visa se quedaba en la decimosexta con
/// 14 archivos. No es un problema de tamaño —ADR-0015 ya fijó que el tamaño es
/// honesto— sino de que un árbol de código es hondo, ancho y uniforme, y gana
/// el concurso de partirse siempre.
#[test]
fn un_proyecto_de_codigo_no_se_parte_y_un_taller_de_proyectos_tampoco() {
    let store = base();
    let mut rutas: Vec<String> = Vec::new();
    for repo in ["uno", "dos"] {
        for sub in ["src", "docs", "dist"] {
            for i in 0..4 {
                rutas.push(format!(r"C:\vida\GitHub\{repo}\{sub}\f{i}.rs"));
            }
        }
    }
    rutas.push(r"C:\vida\visa\ingresos.pdf".into());
    let r: Vec<&str> = rutas.iter().map(String::as_str).collect();
    let files = arbol(&store, &r);
    for f in &files {
        con_formato(&store, f);
    }
    faceta(&store, r"C:\vida\GitHub\uno", "project");
    faceta(&store, r"C:\vida\GitHub\dos", "project");

    // Se piden muchas más regiones de las que este árbol debería soltar.
    let c = sky::regions(&store, 12, at()).unwrap();
    let e = etiquetas(&c);

    assert!(
        !e.iter()
            .any(|l| l.contains("src") || l.contains("dist") || l.contains("docs")),
        "nada de dentro de un repositorio se gana una estrella: {e:?}"
    );
    assert!(
        e.iter().any(|l| l.contains("GitHub")),
        "el taller entero sale una vez: {e:?}"
    );
    assert!(
        e.iter().any(|l| l.contains("visa")),
        "y la vida del usuario recupera su sitio: {e:?}"
    );
    // Y siguen cubiertos todos: no se ha escondido nada, sólo agrupado.
    assert_eq!(c.regions.iter().map(|r| r.weight).sum::<usize>(), 25);
}

/// **Una carpeta cuyo subárbol entero son formatos que el sistema no reconoce
/// no gasta una plaza del cielo.**
///
/// Es la otra mitad del mismo problema, y sólo apareció al arreglar la primera:
/// al dejar de partir los repositorios se liberaron quince plazas, y el reparto
/// se metió a repartirlas **dentro de las partidas guardadas** —`GTA V/Profiles`,
/// `twinkle/kvstore`, `Launcher/Profiles`—. El cielo quedó con más juegos que
/// antes de tocarlo.
///
/// No se excluye nada: los archivos siguen en el índice y en el explorador, y su
/// peso se queda agregado en la carpeta de arriba.
#[test]
fn una_carpeta_sin_un_solo_formato_reconocido_no_gasta_estrella() {
    let store = base();
    let files = arbol(
        &store,
        &[
            r"C:\vida\papeles\visa.pdf",
            r"C:\vida\papeles\banco.pdf",
            r"C:\vida\juego\perfil\PROFSAVE",
            r"C:\vida\juego\perfil\PROFSAVE_backup",
            r"C:\vida\juego\cache\kv_uuid",
        ],
    );
    // Sólo los papeles tienen formato: es exactamente lo que pasa en el disco.
    con_formato(&store, &files[0]);
    con_formato(&store, &files[1]);

    let c = sky::regions(&store, 8, at()).unwrap();
    let e = etiquetas(&c);
    assert!(
        !e.iter()
            .any(|l| l.contains("perfil") || l.contains("cache")),
        "el almacén interno de un programa no es un sitio de una vida: {e:?}"
    );
    assert!(e.iter().any(|l| l.contains("papeles")), "{e:?}");
    // Y los tres archivos mudos siguen cubiertos por alguna región.
    assert_eq!(c.regions.iter().map(|r| r.weight).sum::<usize>(), 5);
}

/// **La guarda.** Sin M2 ejecutado no hay un solo `mime_guess`, y entonces la
/// regla de arriba no puede afirmar nada.
///
/// Distinguir «miré y no encontré» de «no miré nunca» es lo que evita que un
/// índice recién escaneado y sin extraer se vea igual que uno lleno de basura:
/// sin esta guarda el cielo entero sería mudo y no se abriría ni una región.
#[test]
fn sin_ningun_formato_conocido_la_regla_del_mudo_no_se_aplica() {
    let store = base();
    arbol(&store, &[r"C:\vida\uno\a.bin", r"C:\vida\dos\b.bin"]);
    let c = sky::regions(&store, 2, at()).unwrap();
    let mut e = etiquetas(&c);
    e.sort_unstable();
    assert_eq!(e, vec!["dos", "uno"], "se abre igual que siempre");
}

/// **La regla del cielo.** Se empieza por la raíz —una sola estrella que no
/// dice nada— y se parte repetidamente la mayor. Es el gesto de acercarse en un
/// mapa.
#[test]
fn el_cielo_se_abre_partiendo_siempre_la_region_mas_grande() {
    let store = base();
    let mut rutas: Vec<String> = (0..12)
        .map(|i| format!("C:\\vida\\trabajo\\p{i}.txt"))
        .collect();
    rutas.push("C:\\vida\\viajes\\barcelona\\foto.jpg".into());
    rutas.push("C:\\vida\\viajes\\roma\\foto.jpg".into());
    let r: Vec<&str> = rutas.iter().map(String::as_str).collect();
    arbol(&store, &r);

    // Con una sola región: la raíz entera, que no informa de nada.
    let una = sky::regions(&store, 1, at()).unwrap();
    assert_eq!(etiquetas(&una), vec!["vida"]);
    assert_eq!(una.regions[0].weight, 14);

    // Con dos: se parte por donde el árbol se bifurca de verdad.
    let dos = sky::regions(&store, 2, at()).unwrap();
    assert_eq!(etiquetas(&dos), vec!["trabajo", "viajes"]);

    // Con tres: se abre la mayor de las dos... que es trabajo, y no tiene
    // hijas. Así que se abre viajes, que sí.
    //
    // `barcelona` y `roma` pesan **lo mismo**, así que entre ellas decide el
    // desempate por identificador. `regions` promete que *«dos ejecuciones sobre
    // la misma base abren exactamente el mismo cielo»* y lo cumple —los ids ya
    // están escritos—, pero este fixture crea ids nuevos cada vez y
    // `ulid::Ulid::new()` no es monótono dentro de un mismo milisegundo. Fijar
    // aquí un orden entre las dos probaba la suerte del reloj: fallaba una de
    // cada seis ejecuciones. Lo que el cielo promete es **cuáles** se abren.
    let tres = sky::regions(&store, 3, at()).unwrap();
    let e = etiquetas(&tres);
    assert_eq!(e[0], "trabajo", "la mayor sigue mandando: {e:?}");
    let mut empatadas = e[1..].to_vec();
    empatadas.sort_unstable();
    assert_eq!(empatadas, vec!["barcelona", "roma"]);
}

/// Coger «las N carpetas con más archivos» daría siempre la raíz y dentro de
/// ella la misma raíz: una carpeta con una sola hija pesa lo mismo que su hija.
/// Partir por el mayor atraviesa esos pasillos sin detenerse en ellos.
#[test]
fn un_pasillo_de_carpetas_no_gasta_regiones() {
    let store = base();
    arbol(
        &store,
        &[r"C:\a\b\c\izquierda\x.txt", r"C:\a\b\c\derecha\y.txt"],
    );

    let c = sky::regions(&store, 2, at()).unwrap();
    // Ordenado para comparar: las dos pesan lo mismo y el desempate es por
    // identificador, que es orden de creación y no alfabético. Lo que este test
    // afirma es *cuáles* son las regiones, no en qué orden empatan.
    let mut vistas = etiquetas(&c);
    vistas.sort_unstable();
    assert_eq!(
        vistas,
        vec!["derecha", "izquierda"],
        "las dos hojas, no `a` ni `b` ni `c`, que no separan nada"
    );
}

/// Al partir una carpeta que **también** tiene archivos propios, esos archivos
/// no pueden quedarse sin sitio.
#[test]
fn lo_que_cuelga_directamente_de_una_carpeta_partida_sobrevive_como_resto() {
    let store = base();
    arbol(
        &store,
        &[
            r"C:\vida\suelto1.txt",
            r"C:\vida\suelto2.txt",
            r"C:\vida\trabajo\a.txt",
            r"C:\vida\viajes\b.jpg",
        ],
    );

    let c = sky::regions(&store, 3, at()).unwrap();
    let resto = c
        .regions
        .iter()
        .find(|r| r.remainder)
        .expect("tiene que haber una region de resto");
    assert_eq!(resto.label, "vida");
    assert_eq!(resto.weight, 2, "sus dos archivos propios, no los cuatro");
    assert_eq!(c.covered, 4, "y no se pierde ninguno");
}

/// Ningún archivo puede quedarse fuera del mapa. Si la cobertura no es total,
/// hay una parte de la vida que no se dibuja y nadie lo sabría.
#[test]
fn todos_los_archivos_caen_en_alguna_region() {
    let store = base();
    arbol(
        &store,
        &[
            r"C:\vida\a\1.txt",
            r"C:\vida\a\b\2.txt",
            r"C:\vida\a\b\c\3.txt",
            r"C:\vida\otro\4.txt",
            r"C:\vida\5.txt",
        ],
    );

    for objetivo in [1, 2, 3, 5, 10, 50] {
        let c = sky::regions(&store, objetivo, at()).unwrap();
        assert_eq!(
            c.covered, 5,
            "con objetivo {objetivo} se pierde algun archivo"
        );
        assert_eq!(c.files, 5);
        let suma: usize = c.regions.iter().map(|r| r.weight).sum();
        assert!(
            suma >= 5,
            "los pesos tienen que cubrir el corpus: {suma} con objetivo {objetivo}"
        );
    }
}

/// La misma base abre el mismo cielo. Sin esto, el mapa se reordena solo entre
/// dos aperturas y la memoria espacial de §6 no existe.
#[test]
fn el_mismo_grafo_abre_el_mismo_cielo() {
    let store = base();
    arbol(
        &store,
        &[
            r"C:\v\a\1.txt",
            r"C:\v\b\2.txt",
            r"C:\v\c\3.txt",
            r"C:\v\d\4.txt",
        ],
    );

    let a = sky::regions(&store, 3, at()).unwrap();
    let b = sky::regions(&store, 3, at()).unwrap();
    assert_eq!(etiquetas(&a), etiquetas(&b));
    assert_eq!(
        a.regions.iter().map(|r| r.seed).collect::<Vec<_>>(),
        b.regions.iter().map(|r| r.seed).collect::<Vec<_>>()
    );
}

/// Lo que la puerta retiene sube a la región que lo contiene. Es lo que permite
/// saber **dónde no mirar** sin abrir nada.
#[test]
fn una_region_con_algo_en_la_boveda_se_marca_entera() {
    let store = base();
    let files = arbol(
        &store,
        &[
            r"C:\v\claves\.env",
            r"C:\v\claves\otro.txt",
            r"C:\v\normal\a.txt",
        ],
    );
    store
        .set_sensitivity(&files[0], Sensitivity::Vault, 1)
        .unwrap();

    let c = sky::regions(&store, 2, at()).unwrap();
    let claves = c.regions.iter().find(|r| r.label == "claves").unwrap();
    let normal = c.regions.iter().find(|r| r.label == "normal").unwrap();
    assert_eq!(claves.sensitivity, "vault");
    assert_eq!(normal.sensitivity, "normal");
}

// ─────────────────────────────────────────────────────────────── los puentes

/// **Las sesiones no son lugares: son puentes.**
///
/// El árbol de carpetas es *dónde* están las cosas y las sesiones son *cuándo*
/// se tocaron. Mezclar los dos ejes en una vista da un mapa ilegible; cruzarlos
/// da los «Puentes» que `EXPERIENCE.md` §6 sitúa en el mes 1–3.
#[test]
fn una_sesion_que_cruza_dos_carpetas_es_una_arista_entre_regiones() {
    let store = base();
    let files = arbol(
        &store,
        &[r"C:\v\codigo\a.rs", r"C:\v\docs\b.md", r"C:\v\docs\c.md"],
    );

    // Una tarde en la que se tocó código y documentación a la vez.
    let sesion = store.create_entity(T).unwrap();
    store
        .assert(
            NewAssertion::new(
                sesion.clone(),
                "session_span",
                Value::Timestamp(T),
                Method::Deterministic,
                T,
            )
            .valid(Some(T), Some(T + 3_600_000)),
        )
        .unwrap();
    for f in &files {
        store
            .assert(
                NewAssertion::new(
                    f.clone(),
                    "occurred_during",
                    Value::Entity(sesion.clone()),
                    Method::Inferred,
                    T,
                )
                .confidence(0.7),
            )
            .unwrap();
    }

    let c = sky::regions(&store, 2, at()).unwrap();
    assert_eq!(etiquetas(&c), vec!["docs", "codigo"]);
    assert!(
        !c.regions.iter().any(|r| r.label.starts_with("sesión")),
        "una sesion no es una region: no es un lugar"
    );
    assert_eq!(
        c.bridges.len(),
        1,
        "una sesion cruzando dos regiones es un puente"
    );
    assert_eq!(c.bridges[0].weight, 1);

    let extremos: Vec<&str> = c
        .regions
        .iter()
        .filter(|r| r.id == c.bridges[0].from || r.id == c.bridges[0].to)
        .map(|r| r.label.as_str())
        .collect();
    assert_eq!(extremos.len(), 2, "el puente une dos regiones del cielo");
}

/// Una sesión que ocurrió entera dentro de una sola región no es un puente:
/// no cruza nada. Dibujarla sería una arista de un nodo consigo mismo.
#[test]
fn una_sesion_dentro_de_una_sola_region_no_es_un_puente() {
    let store = base();
    let files = arbol(
        &store,
        &[r"C:\v\docs\a.md", r"C:\v\docs\b.md", r"C:\v\otro\c.txt"],
    );

    let sesion = store.create_entity(T).unwrap();
    for f in &files[..2] {
        store
            .assert(NewAssertion::new(
                f.clone(),
                "occurred_during",
                Value::Entity(sesion.clone()),
                Method::Inferred,
                T,
            ))
            .unwrap();
    }

    let c = sky::regions(&store, 2, at()).unwrap();
    assert!(c.bridges.is_empty());
}

// ──────────────────────────────────────────────── el mismo arbol, al ras

/// **Lo que hacía inútil la búsqueda.** Doce archivos en fila son casi siempre
/// tres cosas, y el sistema ya sabía cuáles desde M3.
#[test]
fn los_resultados_salen_agrupados_por_la_carpeta_que_comparten() {
    let store = base();
    let files = arbol(
        &store,
        &[
            r"C:\v\guard\gate.rs",
            r"C:\v\guard\rules.rs",
            r"C:\v\docs\adr.md",
            r"C:\v\guard\content.rs",
        ],
    );
    for (f, texto) in files.iter().zip([
        "sensibilidad de la entidad",
        "reglas de sensibilidad",
        "donde vive la sensibilidad",
        "sensibilidad por contenido",
    ]) {
        obrain_extract::index::replace_chunks(
            store.conn(),
            f,
            "paragraph-pack",
            1,
            &[obrain_extract::chunk::Chunk {
                ordinal: 0,
                byte_start: 0,
                byte_end: texto.len(),
                text: texto.into(),
            }],
        )
        .unwrap();
    }

    let brain = Brain::from_store(store);
    let r = brain.search("sensibilidad", 10).unwrap();

    assert_eq!(r.hits.len(), 4, "cuatro archivos...");
    assert_eq!(r.clusters.len(), 2, "...pero dos cosas");

    let guard = r.clusters.iter().find(|c| c.label == "guard").unwrap();
    assert_eq!(guard.members.len(), 3);
    let docs = r.clusters.iter().find(|c| c.label == "docs").unwrap();
    assert_eq!(docs.members.len(), 1);

    // Y no duplica nada: los grupos guardan identificadores.
    let en_grupos: usize = r.clusters.iter().map(|c| c.members.len()).sum();
    assert_eq!(en_grupos, r.hits.len());
}

/// El orden de los grupos sale del mejor puesto que alcanzó cualquiera de sus
/// miembros. Sin puntuación agregada: una media de puntuaciones no significa
/// nada y hunde al grupo que contiene el resultado número uno.
#[test]
fn el_grupo_del_mejor_resultado_va_primero() {
    let store = base();
    let files = arbol(
        &store,
        &[
            r"C:\v\raro\exacto.txt",
            r"C:\v\comun\a.txt",
            r"C:\v\comun\b.txt",
        ],
    );
    for (f, texto) in files.iter().zip([
        "expediente expediente expediente",
        "menciona expediente una vez entre muchas otras palabras de relleno",
        "otra mencion de expediente entre relleno y mas relleno todavia",
    ]) {
        obrain_extract::index::replace_chunks(
            store.conn(),
            f,
            "paragraph-pack",
            1,
            &[obrain_extract::chunk::Chunk {
                ordinal: 0,
                byte_start: 0,
                byte_end: texto.len(),
                text: texto.into(),
            }],
        )
        .unwrap();
    }

    let brain = Brain::from_store(store);
    let r = brain.search("expediente", 10).unwrap();

    assert_eq!(
        r.clusters[0].best, 1,
        "el primer grupo contiene el resultado numero uno"
    );
    assert_eq!(r.clusters[0].label, "raro");
}

/// Un archivo sin carpeta no desaparece de la lista: cae en su propio grupo.
#[test]
fn lo_que_no_tiene_carpeta_no_se_pierde() {
    let store = base();
    let suelto = store.create_entity(T).unwrap();
    store
        .assert(NewAssertion::new(
            suelto.clone(),
            "path_observed_at",
            "suelto.txt",
            Method::Deterministic,
            T,
        ))
        .unwrap();
    obrain_extract::index::replace_chunks(
        store.conn(),
        &suelto,
        "paragraph-pack",
        1,
        &[obrain_extract::chunk::Chunk {
            ordinal: 0,
            byte_start: 0,
            byte_end: 9,
            text: "expediente".into(),
        }],
    )
    .unwrap();

    let brain = Brain::from_store(store);
    let r = brain.search("expediente", 10).unwrap();
    assert_eq!(r.clusters.len(), 1);
    assert_eq!(r.clusters[0].label, "sin carpeta");
    assert!(r.clusters[0].hub.is_none());
}

/// **El objetivo es un techo, no una cuota.**
///
/// Treinta regiones son las adecuadas para decenas de miles de archivos. Para
/// cien son treinta etiquetas encima de carpetas de tres archivos — exactamente
/// el ruido que la agregación existe para evitar. Se vio pidiendo 30 sobre un
/// corpus de 101: salían 30 regiones y 223 puentes, ilegible.
#[test]
fn el_numero_de_regiones_se_adapta_al_tamano_del_corpus() {
    let store = base();
    let rutas: Vec<String> = (0..100)
        .map(|i| format!("C:\\v\\c{}\\f{i}.txt", i % 20))
        .collect();
    let r: Vec<&str> = rutas.iter().map(String::as_str).collect();
    arbol(&store, &r);

    let c = sky::regions(&store, 30, at()).unwrap();
    assert!(
        c.regions.len() <= 14,
        "con 100 archivos no se abren 30 regiones: salieron {}",
        c.regions.len()
    );
    assert!(
        c.regions.len() >= 8,
        "pero tampoco una: salieron {}",
        c.regions.len()
    );
    assert_eq!(c.covered, 100, "y siguen cabiendo todos");
}

/// Una base vacía abre un cielo vacío, no uno roto.
#[test]
fn un_grafo_vacio_es_un_cielo_vacio() {
    let c = sky::regions(&base(), sky::REGIONS, at()).unwrap();
    assert!(c.regions.is_empty());
    assert!(c.bridges.is_empty());
    assert_eq!(c.files, 0);
    assert_eq!(c.covered, 0);
}
