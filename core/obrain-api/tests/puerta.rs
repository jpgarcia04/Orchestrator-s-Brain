//! Las tres herramientas de la puerta de Aetheris (ADR-0038, ADR-0039 paso
//! 3), de punta a punta: una biblioteca sintética se indexa como la indexa la
//! ventana, se marca una carpeta, se cierra, y la puerta la lee con su
//! apertura de sólo lectura.
//!
//! Sólo archivos inventados en carpetas temporales; ninguna prueba abre la
//! bóveda real.

use obrain_api::lectura::abrir_para_leer;
use obrain_api::puerta::{self, AVISO, NoSale, Posicion};
use obrain_api::seguridad::abrir_con;
use obrain_api::{Brain, indexar, marks};
use obrain_core::{EntityId, Lectura, Sensitivity, Store, TimePoint};
use obrain_keys::KdfParams;
use std::path::PathBuf;

const K: KdfParams = KdfParams::FAST_FOR_TESTS;

/// Una palabra que sólo está dentro del texto de un documento normal: nunca
/// en su nombre, en su ruta ni en una colección.
const SOLO_EN_EL_TEXTO: &str = "zanahoriológico";

const RECIBO: &str = "Recibo sintético de la academia: pagado en azafrán. La palabra \
zanahoriológico sólo aparece aquí dentro.";
const PASAPORTE: &str =
    "Pasaporte sintético de Ana Ruiz, número X0000000, con azafrán en el sello.";

struct Prueba {
    raiz: PathBuf,
}

impl Prueba {
    fn nueva(nombre: &str) -> Self {
        let raiz = std::env::temp_dir().join(format!(
            "obrain-puerta-{nombre}-{}-{}",
            std::process::id(),
            EntityId::new()
        ));
        std::fs::create_dir_all(raiz.join("Biblioteca Sintetica")).unwrap();
        Self { raiz }
    }
    fn biblioteca(&self) -> PathBuf {
        self.raiz.join("Biblioteca Sintetica")
    }
    fn db(&self) -> PathBuf {
        self.raiz.join("datos").join("brain.obrain")
    }
    fn archivo(&self, ruta: &str, texto: &str) {
        let p = self.biblioteca().join(ruta);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, texto).unwrap();
    }
    fn ruta(&self, ruta: &str) -> String {
        std::path::absolute(self.biblioteca().join(ruta))
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for Prueba {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.raiz);
    }
}

fn ahora() -> TimePoint {
    TimePoint::at(obrain_scan::now_ms())
}

/// La biblioteca de siempre, indexada, con `Identidad/` marcada como bóveda
/// y la puerta en «localizar y leer». `Tramites/copia.txt` es el pasaporte
/// otra vez, en una carpeta sin marcar.
fn preparar(p: &Prueba) -> Brain {
    let mut brain = preparar_con_la_puerta_de_fabrica(p);
    brain.set_gate_position(Posicion::LocalizarYLeer).unwrap();
    brain
}

/// Lo mismo, con la puerta como viene de fábrica.
fn preparar_con_la_puerta_de_fabrica(p: &Prueba) -> Brain {
    p.archivo("Normal/recibo.txt", RECIBO);
    p.archivo("Identidad/pasaporte.txt", PASAPORTE);
    p.archivo("Tramites/copia.txt", PASAPORTE);
    p.archivo(
        "Normal/informe.txt",
        "Informe sintético de la práctica, primera versión, sin especias.",
    );
    p.archivo(
        "Normal/informe_v2.txt",
        "Informe sintético de la práctica, segunda versión, también sin especias.",
    );
    std::fs::create_dir_all(p.db().parent().unwrap()).unwrap();
    let mut store = abrir_con(&p.db(), K).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    let informe = indexar::index_with(&mut store, &[p.biblioteca()], None, None).unwrap();
    assert!(informe.errors.is_empty(), "{informe:?}");
    let mut brain = Brain::try_from_store(store).unwrap();
    brain.recovery_code().unwrap();
    brain.confirm_recovery_saved().unwrap();
    let carpeta = marks::folder_at(brain.store(), &p.ruta("Identidad"), ahora())
        .unwrap()
        .expect("la carpeta está en el log");
    brain
        .mark_folder(&carpeta, Some(Sensitivity::Vault))
        .unwrap();
    brain
}

fn id(store: &Store, p: &Prueba, ruta: &str) -> EntityId {
    store
        .find_subject_by_value("path_observed_at", &p.ruta(ruta), ahora())
        .unwrap()
        .unwrap_or_else(|| panic!("{ruta} no está en el log"))
}

/// Lo que hará la puerta en cada llamada: abrir, leer y soltar.
fn por_la_puerta<T>(p: &Prueba, f: impl FnOnce(&Store) -> obrain_api::Result<T>) -> T {
    let l: Lectura = abrir_para_leer(&p.db()).unwrap();
    let mut fuera = None;
    l.leer(|s| {
        fuera = Some(f(s).map_err(|e| obrain_core::Error::Corrupt(e.to_string()))?);
        Ok(())
    })
    .unwrap();
    fuera.unwrap()
}

#[test]
fn localizar_no_lleva_texto_ni_rutas_completas_ni_lo_protegido() {
    let p = Prueba::nueva("localizar");
    let brain = preparar(&p);
    let recibo = id(brain.store(), &p, "Normal/recibo.txt");
    drop(brain);

    let r = por_la_puerta(&p, |s| puerta::localizar(s, None, "azafrán", 10, ahora())).unwrap();
    let ids: Vec<&str> = r.resultados.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(
        ids,
        [recibo.as_str()],
        "ni el pasaporte ni su copia sin marcar"
    );
    let u = &r.resultados[0];
    assert_eq!(u.nombre, "recibo.txt");
    assert_eq!(u.biblioteca, "Biblioteca Sintetica");
    assert_eq!(u.ruta, "Normal/recibo.txt");
    assert_eq!(u.carpeta, "Normal");
    assert!(u.modificado_ms.is_some() && u.bytes == Some(RECIBO.len() as u64));
    assert_eq!(u.ramas, [("bm25".to_string(), 1)]);
    assert_eq!(r.aviso, AVISO);

    // Buscando por la palabra que sólo está en el texto, sale el documento,
    // pero la palabra no viaja en la respuesta, ni la ruta de la biblioteca.
    let r = por_la_puerta(&p, |s| {
        puerta::localizar(s, None, SOLO_EN_EL_TEXTO, 10, ahora())
    })
    .unwrap();
    assert_eq!(r.resultados.len(), 1);
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains(SOLO_EN_EL_TEXTO), "{json}");
    assert!(!json.contains("pagado"), "{json}");
    let raiz = p.raiz.to_string_lossy().replace('\\', "/");
    assert!(!json.replace("\\\\", "/").contains(&raiz), "{json}");
}

#[test]
fn preguntar_por_lo_protegido_responde_lo_mismo_que_por_nada() {
    let p = Prueba::nueva("nada");
    drop(preparar(&p));
    let protegido = por_la_puerta(&p, |s| {
        puerta::localizar(s, None, "pasaporte X0000000", 10, ahora())
    })
    .unwrap();
    let nada = por_la_puerta(&p, |s| puerta::localizar(s, None, "qwxzzyq", 10, ahora())).unwrap();
    assert_eq!(protegido, nada);
    assert!(protegido.resultados.is_empty());
    let leido = por_la_puerta(&p, |s| {
        puerta::leer(s, None, "pasaporte X0000000", 10, ahora())
    })
    .unwrap();
    assert!(leido.pasajes.is_empty());
    assert_eq!(leido.aviso, AVISO);
}

#[test]
fn leer_da_pasajes_de_lo_normal_con_metodo_y_dos_fechas() {
    let p = Prueba::nueva("leer");
    let brain = preparar(&p);
    let recibo = id(brain.store(), &p, "Normal/recibo.txt");
    drop(brain);

    let r = por_la_puerta(&p, |s| puerta::leer(s, None, "azafrán", 10, ahora())).unwrap();
    assert!(!r.pasajes.is_empty());
    for pasaje in &r.pasajes {
        assert_eq!(pasaje.documento, recibo.as_str());
        assert_eq!(pasaje.nombre, "recibo.txt");
        assert_eq!(pasaje.metodo, "bm25");
        assert!(pasaje.leido_ms.is_some() && pasaje.modificado_ms.is_some());
        assert!(
            RECIBO.contains(pasaje.texto.trim_matches('…').trim()),
            "{pasaje:?}"
        );
        assert!(!pasaje.texto.contains("X0000000"));
    }
}

#[test]
fn documento_responde_igual_a_lo_protegido_a_su_copia_a_lo_borrado_y_a_lo_inventado() {
    let p = Prueba::nueva("documento");
    let brain = preparar(&p);
    let pasaporte = id(brain.store(), &p, "Identidad/pasaporte.txt");
    let copia = id(brain.store(), &p, "Tramites/copia.txt");
    let borrado = id(brain.store(), &p, "Normal/informe.txt");
    drop(brain);
    // Se borra y la ventana vuelve a indexar: va a la papelera (ADR-0025).
    std::fs::remove_file(p.biblioteca().join("Normal/informe.txt")).unwrap();
    let mut store = abrir_con(&p.db(), K).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    indexar::index_with(&mut store, &[p.biblioteca()], None, None).unwrap();
    assert!(
        obrain_api::papelera::of(&store, &borrado, ahora())
            .unwrap()
            .is_some()
    );
    drop(store);

    for (que, cual) in [
        ("de bóveda", pasaporte.as_str()),
        ("copia sin marcar de algo de bóveda", copia.as_str()),
        ("en la papelera", borrado.as_str()),
        ("inventado", "01SINTETICOINVENTADO0000000"),
        ("vacío", ""),
    ] {
        let r = por_la_puerta(&p, |s| puerta::documento(s, cual, ahora()));
        assert_eq!(r, Err(NoSale::FueraDeLaPuerta), "{que}");
    }
}

#[test]
fn la_ficha_de_lo_normal_trae_su_ruta_completa_y_ninguna_relacion_inferida() {
    let p = Prueba::nueva("ficha");
    let brain = preparar(&p);
    let v2 = id(brain.store(), &p, "Normal/informe_v2.txt");
    // Que la prueba no pase en vacío: el enlazador sí vio la versión.
    assert!(
        brain
            .store()
            .query(None, Some("version_of"), ahora())
            .unwrap()
            .iter()
            .any(|a| a.subject == v2 || a.object.as_entity() == Some(&v2)),
        "el enlazador de versiones no relacionó informe_v2"
    );
    drop(brain);

    let ficha = por_la_puerta(&p, |s| puerta::documento(s, v2.as_str(), ahora())).unwrap();
    assert_eq!(ficha.nombre, "informe_v2.txt");
    assert_eq!(ficha.ruta, "Normal/informe_v2.txt");
    assert_eq!(ficha.ruta_completa, p.ruta("Normal/informe_v2.txt"));
    assert_eq!(ficha.aviso, AVISO);
    assert!(
        ficha
            .relaciones
            .iter()
            .all(|r| ["carpeta", "copia_de", "dentro_de"].contains(&r.tipo)),
        "{:?}",
        ficha.relaciones
    );
    assert!(
        ficha
            .relaciones
            .iter()
            .any(|r| r.tipo == "carpeta" && r.ruta == "Normal" && r.id.is_empty()),
        "{:?}",
        ficha.relaciones
    );
}

#[test]
fn el_pin_y_el_desbloqueo_de_la_ventana_no_cambian_lo_que_sale() {
    let p = Prueba::nueva("pin");
    let brain = preparar(&p);
    let sin_pin = puerta::localizar(brain.store(), None, "azafrán", 10, ahora())
        .unwrap()
        .unwrap();
    brain.set_pin("2468", None).unwrap();
    let bloqueada = puerta::localizar(brain.store(), None, "azafrán", 10, ahora())
        .unwrap()
        .unwrap();
    brain.unlock("2468").unwrap();
    let desbloqueada = puerta::localizar(brain.store(), None, "azafrán", 10, ahora())
        .unwrap()
        .unwrap();
    assert_eq!(sin_pin, bloqueada);
    assert_eq!(sin_pin, desbloqueada);
    assert_eq!(sin_pin.resultados.len(), 1);
}

/// Un modelo de significado de juguete: lo que habla de especias apunta a un
/// lado y lo demás al otro. Determinista y sin descargar nada.
struct Especias;

impl obrain_api::Embedder for Especias {
    fn model(&self) -> obrain_embed::embedder::ModelId {
        obrain_embed::embedder::ModelId {
            id: "especias-de-prueba",
            version: 1,
            dim: 2,
        }
    }

    fn embed(
        &self,
        texts: &[&str],
        _role: obrain_embed::embedder::Role,
    ) -> obrain_embed::Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| {
                let t = t.to_lowercase();
                if t.contains("azafr") || t.contains("especia dorada") {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                }
            })
            .collect())
    }
}

#[test]
fn por_significado_tambien_solo_sale_lo_normal_y_el_pasaje_dice_knn() {
    let p = Prueba::nueva("significado");
    let brain = preparar(&p);
    let recibo = id(brain.store(), &p, "Normal/recibo.txt");
    drop(brain);
    // Vectores para todo lo que la ventana deja vectorizar (lo de bóveda no).
    let mut store = abrir_con(&p.db(), K).unwrap();
    Brain::initialize_store(&mut store).unwrap();
    indexar::index_with(&mut store, &[p.biblioteca()], Some(&Especias), None).unwrap();
    drop(store);

    // Ninguna palabra de la consulta está en ningún texto: sólo el
    // significado lo encuentra. La copia sin marcar del pasaporte también
    // habla de azafrán, tiene vector y no sale.
    let r = por_la_puerta(&p, |s| {
        puerta::localizar(s, Some(&Especias), "especia dorada", 10, ahora())
    })
    .unwrap();
    assert!(r.semantica);
    let ids: Vec<&str> = r.resultados.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids.first().copied(), Some(recibo.as_str()), "{r:?}");
    let copia = por_la_puerta(&p, |s| Ok(id(s, &p, "Tramites/copia.txt")));
    assert!(!ids.contains(&copia.as_str()), "{r:?}");
    assert!(r.resultados[0].ramas.iter().any(|(rama, _)| rama == "knn"));

    let l = por_la_puerta(&p, |s| {
        puerta::leer(s, Some(&Especias), "especia dorada", 10, ahora())
    })
    .unwrap();
    let del_recibo: Vec<_> = l
        .pasajes
        .iter()
        .filter(|x| x.documento == recibo.as_str())
        .collect();
    assert!(!del_recibo.is_empty(), "{l:?}");
    assert!(del_recibo.iter().all(|x| x.metodo == "knn"), "{l:?}");
    assert!(l.pasajes.iter().all(|x| x.documento != copia.as_str()));
}

/// Lo que contesta cada herramienta por la puerta, en el orden localizar,
/// leer, documento: `true` si dio respuesta, `false` si dijo `apagada`.
fn que_contesta(p: &Prueba, recibo: &EntityId) -> [bool; 3] {
    let dio = |r: Result<bool, NoSale>| match r {
        Ok(_) => true,
        Err(NoSale::Apagada) => false,
        Err(otro) => panic!("esperaba respuesta o apagada, no {otro}"),
    };
    let localizar = por_la_puerta(p, |s| {
        Ok(puerta::localizar(s, None, "azafrán", 10, ahora())?.map(|r| !r.resultados.is_empty()))
    });
    let leer = por_la_puerta(p, |s| {
        Ok(puerta::leer(s, None, "azafrán", 10, ahora())?.map(|r| !r.pasajes.is_empty()))
    });
    let documento = por_la_puerta(p, |s| {
        Ok(puerta::documento(s, recibo.as_str(), ahora())?.map(|f| f.id == recibo.as_str()))
    });
    for r in [localizar, leer, documento] {
        // Cuando contesta, contesta de verdad: no una lista vacía.
        assert_ne!(r, Ok(false));
    }
    [dio(localizar), dio(leer), dio(documento)]
}

#[test]
fn la_puerta_viene_apagada_y_cada_posicion_deja_lo_suyo() {
    let p = Prueba::nueva("posiciones");
    let mut brain = preparar_con_la_puerta_de_fabrica(&p);
    let recibo = id(brain.store(), &p, "Normal/recibo.txt");
    assert_eq!(brain.gate_position().unwrap(), Posicion::Apagada);
    assert_eq!(que_contesta(&p, &recibo), [false, false, false]);

    brain.set_gate_position(Posicion::SoloLocalizar).unwrap();
    assert_eq!(que_contesta(&p, &recibo), [true, false, false]);
    // Con «sólo localizar», un identificador inventado también dice
    // `apagada`: la herramienta no llega a mirar qué es.
    let inventado = por_la_puerta(&p, |s| puerta::documento(s, "01INVENTADO", ahora()));
    assert_eq!(inventado, Err(NoSale::Apagada));

    brain.set_gate_position(Posicion::LocalizarYLeer).unwrap();
    assert_eq!(brain.gate_position().unwrap(), Posicion::LocalizarYLeer);
    assert_eq!(que_contesta(&p, &recibo), [true, true, true]);

    brain.set_gate_position(Posicion::Apagada).unwrap();
    assert_eq!(que_contesta(&p, &recibo), [false, false, false]);
}

#[test]
fn abrir_la_puerta_pide_el_pin_y_cerrarla_nunca() {
    let p = Prueba::nueva("interruptor");
    let mut brain = preparar_con_la_puerta_de_fabrica(&p);
    brain.set_pin("2468", None).unwrap();
    brain.lock();
    assert!(matches!(
        brain.set_gate_position(Posicion::SoloLocalizar),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert_eq!(brain.gate_position().unwrap(), Posicion::Apagada);

    brain.unlock("2468").unwrap();
    brain.set_gate_position(Posicion::SoloLocalizar).unwrap();
    brain.lock();
    assert!(matches!(
        brain.set_gate_position(Posicion::LocalizarYLeer),
        Err(obrain_api::Error::Bloqueado)
    ));
    assert_eq!(brain.gate_position().unwrap(), Posicion::SoloLocalizar);

    // Bloqueada, cerrar sí: de «sólo localizar» a apagada.
    brain.set_gate_position(Posicion::Apagada).unwrap();
    assert_eq!(brain.gate_position().unwrap(), Posicion::Apagada);
}

#[test]
fn un_ajuste_que_no_se_entiende_deja_la_puerta_apagada() {
    let p = Prueba::nueva("ajuste-raro");
    let brain = preparar_con_la_puerta_de_fabrica(&p);
    let recibo = id(brain.store(), &p, "Normal/recibo.txt");
    for raro in ["abierta", "LOCALIZAR_Y_LEER", " localizar_y_leer", ""] {
        obrain_api::seguridad::poner_ajuste(brain.store().conn(), Posicion::CLAVE, raro).unwrap();
        assert_eq!(
            brain.gate_position().unwrap(),
            Posicion::Apagada,
            "{raro:?}"
        );
        assert_eq!(que_contesta(&p, &recibo), [false, false, false], "{raro:?}");
    }
}
