//! **Arma la base desechable de la batería de ataques de Aetheris** (ADR-0039,
//! paso 6). No es una prueba: es un ayudante que sólo corre cuando lo lanza
//! la batería, con `--ignored` y `OBRAIN_BATERIA` apuntando a un manifiesto:
//!
//! ```json
//! {
//!   "biblioteca": "/tmp/…/Biblioteca",
//!   "base": "/tmp/…/datos/brain.obrain",
//!   "colecciones": { "Seguros": ["Seguros/poliza.txt"] },
//!   "puerta": "localizar_y_leer",
//!   "modelo": false,
//!   "informe": "/tmp/…/informe-de-brain.json"
//! }
//! ```
//!
//! Indexa la biblioteca como la indexa la ventana, deja que **el clasificador
//! de verdad** decida qué es sensible o de bóveda (eso es lo que la batería
//! mide, casos A10 y B1), crea las colecciones y mueve el interruptor con la
//! misma llamada que la ventana ([`Brain::set_gate_position`]). Escribe en
//! `informe` el nivel y el estado de publicación que Brain dio a cada archivo.
//!
//! Sólo crea una base nueva: se niega si ya existe algo en `base`, o si
//! `base` es la ruta de la base de verdad. Los documentos de la batería son
//! texto hostil a propósito y nunca deben entrar en la bóveda de JP.

use obrain_api::puerta::Posicion;
use obrain_api::{Brain, indexar, seguridad};
use obrain_core::{EntityId, TimePoint};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifiesto {
    biblioteca: PathBuf,
    base: PathBuf,
    #[serde(default)]
    colecciones: BTreeMap<String, Vec<String>>,
    puerta: Posicion,
    #[serde(default)]
    modelo: bool,
    informe: PathBuf,
}

fn ahora() -> TimePoint {
    TimePoint::at(obrain_scan::now_ms())
}

/// `relativa` viene con `/`; se arma pieza a pieza para que en Windows quede
/// con `\`, como la guarda el indexador.
fn ruta_absoluta(biblioteca: &Path, relativa: &str) -> String {
    let ruta = relativa
        .split('/')
        .fold(biblioteca.to_path_buf(), |r, p| r.join(p));
    std::path::absolute(ruta)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
#[ignore = "ayudante de la batería de Aetheris: necesita OBRAIN_BATERIA"]
fn base_para_la_bateria_de_aetheris() {
    let manifiesto =
        std::env::var_os("OBRAIN_BATERIA").expect("OBRAIN_BATERIA tiene que apuntar al manifiesto");
    let m: Manifiesto = serde_json::from_slice(&std::fs::read(manifiesto).unwrap()).unwrap();

    // Nunca sobre una base que ya existe, y nunca la de verdad.
    let de_verdad = obrain_core::ubicacion::fija().join(obrain_core::ubicacion::BASE);
    assert_ne!(
        std::path::absolute(&m.base).unwrap(),
        std::path::absolute(&de_verdad).unwrap(),
        "ésa es la base de verdad"
    );
    for sufijo in ["", "-wal", "-shm", "-journal", ".llave"] {
        let mut p = m.base.as_os_str().to_owned();
        p.push(sufijo);
        assert!(
            !Path::new(&p).exists(),
            "ya hay algo en {}: la batería sólo usa una base nueva",
            Path::new(&p).display()
        );
    }
    std::fs::create_dir_all(m.base.parent().unwrap()).unwrap();

    let modelo = if m.modelo {
        obrain_api::load_model(None).unwrap()
    } else {
        None
    };
    let mut store = seguridad::abrir(&m.base).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    let informe = indexar::index_with(
        &mut store,
        std::slice::from_ref(&m.biblioteca),
        modelo.as_deref().map(|e| e as &dyn obrain_api::Embedder),
        None,
    )
    .unwrap();
    assert!(informe.errors.is_empty(), "{informe:?}");
    let mut brain = Brain::try_from_store(store).unwrap();

    let documento = |brain: &Brain, relativa: &str| -> EntityId {
        brain
            .store()
            .find_subject_by_value(
                "path_observed_at",
                &ruta_absoluta(&m.biblioteca, relativa),
                ahora(),
            )
            .unwrap()
            .unwrap_or_else(|| panic!("{relativa} no está indexado"))
    };
    for (nombre, rutas) in &m.colecciones {
        let id = EntityId(brain.create_collection(nombre, "user@bateria").unwrap());
        let docs: Vec<EntityId> = rutas.iter().map(|r| documento(&brain, r)).collect();
        brain.add_to_collection(&id, &docs, "user@bateria").unwrap();
    }
    brain.set_gate_position(m.puerta).unwrap();

    // Lo que decidió Brain de cada archivo, para que la batería lo compare
    // con lo que esperaba.
    let mut niveles = BTreeMap::new();
    for entrada in walk(&m.biblioteca) {
        let relativa = entrada
            .strip_prefix(&m.biblioteca)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let id = documento(&brain, &relativa);
        let nivel = brain.store().sensitivity(&id).unwrap();
        let publicado = brain
            .store()
            .conn()
            .query_row(
                "SELECT state FROM content_publication WHERE entity = ?1",
                [id.as_str()],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "sin_publicacion".into());
        niveles.insert(
            relativa,
            serde_json::json!({ "id": id.as_str(), "nivel": nivel.as_str(), "publicacion": publicado }),
        );
    }
    std::fs::write(
        &m.informe,
        serde_json::to_vec_pretty(&serde_json::json!({
            "puerta": m.puerta.as_str(),
            "semantica": modelo.is_some(),
            "documentos": niveles,
        }))
        .unwrap(),
    )
    .unwrap();
}

/// Los archivos de la biblioteca, en orden.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            v.extend(walk(&p));
        } else {
            v.push(p);
        }
    }
    v.sort();
    v
}
