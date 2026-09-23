//! **La carpeta sigue a su `FileID`** ([ADR-0027]).
//!
//! Los archivos se reconocen por su identidad desde M1 (I8); las carpetas, por
//! su ruta, porque las crea el enlazador de M3 a partir de la ruta de cada
//! archivo. Renombrar una carpeta creaba otra entidad, y la marca de sensible
//! ([ADR-0020]) se quedaba en la vieja protegiendo nada.
//!
//! Aquí las carpetas ganan identidad sin cambiar cómo las crea el enlazador:
//!
//! 1. [`follow`], **antes de enlazar**: una carpeta del log cuyo `folder_id`
//!    aparece en el recorrido bajo otra ruta se muda —retracta su ruta vieja y
//!    afirma la nueva en la misma entidad—. El enlazador la encuentra por la
//!    ruta nueva y todo lo que cuelga de ella (la marca, su sitio en el mapa)
//!    viaja solo.
//! 2. [`identify`], **después de enlazar**: cada carpeta del log con la ruta de
//!    una carpeta recorrida apunta su `folder_id`, para reconocerla la próxima
//!    vez.
//!
//! [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md
//! [ADR-0027]: ../../../docs/adr/0027-la-carpeta-sigue-a-su-fileid.md

use crate::scan::FolderCandidate;
use obrain_core::{AssertionId, EntityId, Facet, Method, NewAssertion, Store, TimePoint, Value};
use std::collections::{HashMap, HashSet};

/// «Identidad de carpeta»: el `(volumen, FileID)` de un directorio, en el mismo
/// formato que el `file_id` de un archivo.
pub const FOLDER_ID: &str = "folder_id";

const SOURCE: &str = "scan@m1";

/// Una mudanza por hacer: la carpeta, su ruta vigente y adónde va.
struct Mudanza {
    folder: EntityId,
    old: AssertionId,
    new_path: String,
}

/// Muda las carpetas que el recorrido ha visto en otra ruta. Devuelve cuántas.
///
/// Los seguros de [ADR-0027] §4, porque mudar de más es peor que no mudar:
///
/// 1. **La ruta vieja tiene que haber desaparecido**: el disco dice *no
///    encontrado*, o el recorrido vio allí otra carpeta. Cambiar sólo
///    mayúsculas también cuenta: en Windows la ruta vieja «existe» porque es
///    la misma.
/// 2. **Una biblioteca no se muda**: es la ruta que eligió el usuario.
/// 3. **Si la ruta nueva ya la tiene otra carpeta** que no se muda, ésa se
///    retira; salvo que tenga una marca del usuario o sea una biblioteca, y
///    entonces no se muda nadie.
///
/// El llamante lo envuelve en la transacción de la ingesta.
pub fn follow(store: &Store, folders: &[FolderCandidate], at: i64) -> obrain_core::Result<usize> {
    let tp = TimePoint::at(at);
    let vistas: HashMap<String, String> = folders
        .iter()
        .map(|f| (f.identity.to_hex(), f.path.to_string_lossy().to_string()))
        .collect();
    let rutas_vistas: HashSet<&str> = vistas.values().map(String::as_str).collect();

    let mut mudanzas: Vec<Mudanza> = Vec::new();
    for a in store.query(None, Some(FOLDER_ID), tp)? {
        let Some(nueva) = a.object.as_text().and_then(|h| vistas.get(h)) else {
            continue;
        };
        let Some(vieja) = store.resolve(&a.subject, "folder_path", tp)? else {
            continue;
        };
        let Some(ruta_vieja) = vieja.object.as_text() else {
            continue;
        };
        if ruta_vieja == nueva || is_library(store, &a.subject, tp)? {
            continue;
        }
        let solo_mayusculas = ruta_vieja.to_lowercase() == nueva.to_lowercase();
        if !solo_mayusculas && !rutas_vistas.contains(ruta_vieja) {
            match std::fs::symlink_metadata(ruta_vieja) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => continue,
            }
        }
        mudanzas.push(Mudanza {
            folder: a.subject,
            old: vieja.id,
            new_path: nueva.clone(),
        });
    }

    // Quién ocupa ya cada ruta nueva. Se decide antes de escribir nada, y hasta
    // que no cambie: una mudanza que se cae deja su ruta vieja ocupada, y eso
    // puede tumbar la de otra carpeta que iba justo ahí.
    let mut ocupantes: HashMap<usize, EntityId> = HashMap::new();
    for (i, m) in mudanzas.iter().enumerate() {
        if let Some(g) = store.find_subject_by_value("folder_path", &m.new_path, tp)? {
            if g != m.folder {
                ocupantes.insert(i, g);
            }
        }
    }
    let mut vigentes: Vec<bool> = vec![true; mudanzas.len()];
    loop {
        let se_van: HashSet<&EntityId> = mudanzas
            .iter()
            .zip(&vigentes)
            .filter(|(_, v)| **v)
            .map(|(m, _)| &m.folder)
            .collect();
        let mut cambio = false;
        for (i, g) in &ocupantes {
            if vigentes[*i] && !se_van.contains(g) && is_protected(store, g, tp)? {
                vigentes[*i] = false;
                cambio = true;
            }
        }
        if !cambio {
            break;
        }
    }

    let se_van: HashSet<EntityId> = mudanzas
        .iter()
        .zip(&vigentes)
        .filter(|(_, v)| **v)
        .map(|(m, _)| m.folder.clone())
        .collect();
    // Primero todas las rutas viejas: renombrar la madre muda a la vez a las
    // hijas, y ninguna tiene que encontrarse a otra en su sitio.
    for (m, _) in mudanzas.iter().zip(&vigentes).filter(|(_, v)| **v) {
        store.retract(&m.old, at)?;
    }
    for (i, m) in mudanzas.iter().enumerate() {
        if !vigentes[i] {
            continue;
        }
        // Una carpeta que ya no existe ocupaba esta ruta: se retira.
        if let Some(g) = ocupantes.get(&i).filter(|g| !se_van.contains(*g)) {
            for pred in ["folder_path", FOLDER_ID] {
                for a in store.query(Some(g), Some(pred), tp)? {
                    store.retract(&a.id, at)?;
                }
            }
        }
        store.assert(
            NewAssertion::new(
                m.folder.clone(),
                "folder_path",
                m.new_path.as_str(),
                Method::Deterministic,
                at,
            )
            .source(SOURCE),
        )?;
    }
    Ok(se_van.len())
}

/// Apunta el `folder_id` de cada carpeta del log que tiene la ruta de una
/// carpeta recorrida. Devuelve cuántas identidades se escribieron.
///
/// Una carpeta que el enlazador todavía no conoce (no tiene nada dentro) no se
/// crea aquí: se identificará el día que el enlazador la cree.
///
/// Si la ruta tiene **otra** identidad —la carpeta se borró y se creó otra con
/// el mismo nombre—, la entidad de esa ruta se queda con la nueva, como un
/// archivo tras un guardado atómico (ADR-0007): una marca de esa ruta sigue
/// protegiendo lo que hay en ella, que es lo seguro.
pub fn identify(store: &Store, folders: &[FolderCandidate], at: i64) -> obrain_core::Result<usize> {
    let tp = TimePoint::at(at);
    let mut n = 0;
    for f in folders {
        let hex = f.identity.to_hex();
        // Ya es de alguien. Si ése está en otra ruta es una mudanza que no se
        // hizo (ver [`follow`]), y no se le quita la identidad.
        if store.find_subject_by_value(FOLDER_ID, &hex, tp)?.is_some() {
            continue;
        }
        let ruta = f.path.to_string_lossy();
        let Some(e) = store.find_subject_by_value("folder_path", &ruta, tp)? else {
            continue;
        };
        for a in store.query(Some(&e), Some(FOLDER_ID), tp)? {
            store.retract(&a.id, at)?;
        }
        store.assert(
            NewAssertion::new(e, FOLDER_ID, hex.as_str(), Method::Deterministic, at).source(SOURCE),
        )?;
        n += 1;
    }
    Ok(n)
}

fn is_library(store: &Store, e: &EntityId, tp: TimePoint) -> obrain_core::Result<bool> {
    store.has_assertion(
        e,
        "has_facet",
        &Value::Text(Facet::Library.as_str().to_string()),
        tp,
    )
}

/// ¿Lleva algo que el usuario decidió —una marca, ser biblioteca— y que no se
/// puede retirar sin preguntarle?
fn is_protected(store: &Store, e: &EntityId, tp: TimePoint) -> obrain_core::Result<bool> {
    if is_library(store, e, tp)? {
        return Ok(true);
    }
    Ok(store
        .query(
            Some(e),
            Some(obrain_core::sensitivity::CONTENTS_SENSITIVITY),
            tp,
        )?
        .iter()
        .any(|a| a.method == Method::User))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::FileIdentity;
    use std::path::PathBuf;

    const T: i64 = 1_700_000_000_000;

    fn id(n: u8) -> FileIdentity {
        let mut file_id = [0u8; 16];
        file_id[0] = n;
        FileIdentity { volume: 7, file_id }
    }

    fn vista(path: &str, n: u8) -> FolderCandidate {
        FolderCandidate {
            path: PathBuf::from(path),
            identity: id(n),
        }
    }

    /// Una carpeta del log con su ruta y, si se da, su identidad.
    fn carpeta(s: &Store, path: &str, n: Option<u8>) -> EntityId {
        let e = s.create_entity(T).unwrap();
        s.assert(NewAssertion::new(
            e.clone(),
            "folder_path",
            path,
            Method::Deterministic,
            T,
        ))
        .unwrap();
        if let Some(n) = n {
            s.assert(NewAssertion::new(
                e.clone(),
                FOLDER_ID,
                id(n).to_hex().as_str(),
                Method::Deterministic,
                T,
            ))
            .unwrap();
        }
        e
    }

    fn marcar(s: &Store, e: &EntityId) {
        s.assert(NewAssertion::new(
            e.clone(),
            obrain_core::sensitivity::CONTENTS_SENSITIVITY,
            "sensitive",
            Method::User,
            T,
        ))
        .unwrap();
    }

    fn ruta(s: &Store, e: &EntityId, at: i64) -> Option<String> {
        s.resolve(e, "folder_path", TimePoint::at(at))
            .unwrap()
            .and_then(|a| a.object.as_text().map(str::to_string))
    }

    // Rutas que no existen en ningún disco: el disco dice «no encontrado».
    const VIEJA: &str = r"Z:\obrain-no-existe\01 Identidad";
    const NUEVA: &str = r"Z:\obrain-no-existe\01 Identidad y viajes";

    #[test]
    fn renombrar_muda_la_misma_carpeta_y_su_marca() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, Some(1));
        marcar(&s, &e);

        assert_eq!(follow(&s, &[vista(NUEVA, 1)], T + 10).unwrap(), 1);
        assert_eq!(ruta(&s, &e, T + 20).as_deref(), Some(NUEVA));
        assert_eq!(
            ruta(&s, &e, T + 5).as_deref(),
            Some(VIEJA),
            "el log recuerda dónde estaba (I4)"
        );

        let marcas = s.folder_marks(TimePoint::at(T + 20)).unwrap();
        let m = marcas.of(&e).expect("la marca sigue en la misma carpeta");
        assert_eq!(m.path, NUEVA);
        assert_eq!(m.asserted_at, T, "con su fecha de siempre");
    }

    #[test]
    fn si_la_ruta_vieja_sigue_en_el_disco_no_se_muda() {
        let s = Store::open_in_memory().unwrap();
        let dir = std::env::temp_dir().join(format!("obrain-carpetas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let vieja = dir.to_string_lossy().to_string();
        let e = carpeta(&s, &vieja, Some(1));

        assert_eq!(follow(&s, &[vista(NUEVA, 1)], T + 10).unwrap(), 0);
        assert_eq!(ruta(&s, &e, T + 20), Some(vieja));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn si_en_la_ruta_vieja_hay_otra_carpeta_si_se_muda() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, Some(1));
        let vistas = [vista(NUEVA, 1), vista(VIEJA, 2)];
        assert_eq!(follow(&s, &vistas, T + 10).unwrap(), 1);
        assert_eq!(ruta(&s, &e, T + 20).as_deref(), Some(NUEVA));
    }

    #[test]
    fn cambiar_solo_mayusculas_tambien_es_mudarse() {
        let s = Store::open_in_memory().unwrap();
        // Una ruta que sí existe: en Windows, la de mayúsculas cambiadas
        // también «existe».
        let dir = std::env::temp_dir().join(format!("obrain-mayus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let actual = dir.to_string_lossy().to_string();
        let antes = actual.to_uppercase();
        let e = carpeta(&s, &antes, Some(1));
        assert_eq!(follow(&s, &[vista(&actual, 1)], T + 10).unwrap(), 1);
        assert_eq!(ruta(&s, &e, T + 20), Some(actual));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn renombrar_la_madre_muda_a_las_hijas() {
        let s = Store::open_in_memory().unwrap();
        let madre = carpeta(&s, r"Z:\obrain-no-existe\Bóveda\Docs", Some(1));
        let hija = carpeta(&s, r"Z:\obrain-no-existe\Bóveda\Docs\01 Identidad", Some(2));
        let vistas = [
            vista(r"Z:\obrain-no-existe\Bóveda\Papeles", 1),
            vista(r"Z:\obrain-no-existe\Bóveda\Papeles\01 Identidad", 2),
        ];
        assert_eq!(follow(&s, &vistas, T + 10).unwrap(), 2);
        assert_eq!(
            ruta(&s, &madre, T + 20).as_deref(),
            Some(r"Z:\obrain-no-existe\Bóveda\Papeles")
        );
        assert_eq!(
            ruta(&s, &hija, T + 20).as_deref(),
            Some(r"Z:\obrain-no-existe\Bóveda\Papeles\01 Identidad")
        );
    }

    #[test]
    fn una_biblioteca_no_se_muda() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, Some(1));
        s.assert(NewAssertion::new(
            e.clone(),
            "has_facet",
            Facet::Library.as_str(),
            Method::User,
            T,
        ))
        .unwrap();
        assert_eq!(follow(&s, &[vista(NUEVA, 1)], T + 10).unwrap(), 0);
        assert_eq!(ruta(&s, &e, T + 20).as_deref(), Some(VIEJA));
    }

    /// Una carpeta borrada hace meses con el nombre al que llega ésta: se
    /// retira, y la que llega ocupa su sitio.
    #[test]
    fn una_carpeta_borrada_en_la_ruta_nueva_se_retira() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, Some(1));
        marcar(&s, &e);
        let fantasma = carpeta(&s, NUEVA, None);

        assert_eq!(follow(&s, &[vista(NUEVA, 1)], T + 10).unwrap(), 1);
        assert_eq!(ruta(&s, &e, T + 20).as_deref(), Some(NUEVA));
        assert_eq!(ruta(&s, &fantasma, T + 20), None);
        assert_eq!(
            s.find_subject_by_value("folder_path", NUEVA, TimePoint::at(T + 20))
                .unwrap(),
            Some(e),
            "una sola carpeta con esa ruta"
        );
    }

    /// …salvo que la borrada estuviera marcada: su marca sigue cubriendo esa
    /// ruta como hasta hoy, y no se muda nadie.
    #[test]
    fn una_carpeta_marcada_en_la_ruta_nueva_no_se_toca() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, Some(1));
        let marcada = carpeta(&s, NUEVA, None);
        marcar(&s, &marcada);

        assert_eq!(follow(&s, &[vista(NUEVA, 1)], T + 10).unwrap(), 0);
        assert_eq!(ruta(&s, &e, T + 20).as_deref(), Some(VIEJA));
        assert_eq!(ruta(&s, &marcada, T + 20).as_deref(), Some(NUEVA));
        // Y la identidad no se le pasa a la otra.
        assert_eq!(identify(&s, &[vista(NUEVA, 1)], T + 30).unwrap(), 0);
    }

    /// Dos carpetas que se intercambian el nombre: las dos se mudan.
    #[test]
    fn dos_carpetas_que_se_cambian_el_nombre() {
        let s = Store::open_in_memory().unwrap();
        let a = carpeta(&s, VIEJA, Some(1));
        let b = carpeta(&s, NUEVA, Some(2));
        marcar(&s, &b);
        let vistas = [vista(NUEVA, 1), vista(VIEJA, 2)];
        assert_eq!(follow(&s, &vistas, T + 10).unwrap(), 2);
        assert_eq!(ruta(&s, &a, T + 20).as_deref(), Some(NUEVA));
        assert_eq!(ruta(&s, &b, T + 20).as_deref(), Some(VIEJA));
    }

    #[test]
    fn identificar_apunta_la_identidad_de_lo_que_ya_existe() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, None);
        let vistas = [vista(VIEJA, 1), vista(NUEVA, 2)];
        assert_eq!(
            identify(&s, &vistas, T + 10).unwrap(),
            1,
            "sólo la que el log conoce: la otra no tiene nada dentro"
        );
        assert_eq!(
            s.find_subject_by_value(FOLDER_ID, &id(1).to_hex(), TimePoint::at(T + 20))
                .unwrap(),
            Some(e)
        );
        assert_eq!(identify(&s, &vistas, T + 30).unwrap(), 0, "idempotente");
    }

    /// Se borró la carpeta y se creó otra con el mismo nombre: la entidad de
    /// la ruta se queda con la identidad nueva, y su marca con ella.
    #[test]
    fn otra_carpeta_con_el_mismo_nombre_hereda_la_ruta() {
        let s = Store::open_in_memory().unwrap();
        let e = carpeta(&s, VIEJA, Some(1));
        assert_eq!(identify(&s, &[vista(VIEJA, 2)], T + 10).unwrap(), 1);
        let ids = s
            .query(Some(&e), Some(FOLDER_ID), TimePoint::at(T + 20))
            .unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].object.as_text(), Some(id(2).to_hex().as_str()));
    }
}
