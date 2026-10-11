//! La papelera de [ADR-0025]: lo que ya no está en el disco sale de todas
//! partes, se queda 30 días en su vista y después se olvida lo que se leyó de
//! él; el log lo recuerda.
//!
//! Todo con el pipeline real (`indexar`) sobre carpetas de verdad: lo que se
//! prueba es lo que pasa cuando borras, mueves o renombras en tu disco.
//!
//! [ADR-0025]: ../../../docs/adr/0025-lo-que-ya-no-esta.md

use obrain_api::{indexar, papelera, sky};
use obrain_core::{EntityId, Method, Store, TimePoint};
use std::path::{Path, PathBuf};

const DIA: i64 = 86_400_000;

struct Boveda {
    dir: PathBuf,
    lib: PathBuf,
    store: Store,
}

impl Boveda {
    fn nueva(nombre: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("obrain-papelera-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lib = dir.join("Boveda");
        std::fs::create_dir_all(lib.join("01 Identidad")).unwrap();
        std::fs::create_dir_all(lib.join("02 Salud")).unwrap();
        std::fs::write(
            lib.join("01 Identidad/pasaporte.txt"),
            "Pasaporte mexicano. Fecha de caducidad: 12/03/2031",
        )
        .unwrap();
        std::fs::write(
            lib.join("02 Salud/poliza.txt"),
            "Póliza de salud. Vigencia hasta el 31/08/2031.",
        )
        .unwrap();
        std::fs::write(
            lib.join("02 Salud/receta.txt"),
            "Receta médica de ibuprofeno.",
        )
        .unwrap();
        let mut b = Boveda {
            dir,
            lib,
            store: Store::open_in_memory().unwrap(),
        };
        b.indexar();
        b
    }

    fn indexar(&mut self) -> obrain_api::IndexReport {
        indexar::index_with(&mut self.store, std::slice::from_ref(&self.lib), None, None).unwrap()
    }

    /// `rel` con `/`, que en Windows se convierte en `\`: el escáner guarda
    /// la ruta con el separador del sistema, y buscarla con otro no la
    /// encuentra.
    fn ruta(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.lib.clone(), |p, t| p.join(t))
    }

    fn at(&self) -> TimePoint {
        TimePoint::at(obrain_scan::now_ms())
    }

    fn recientes(&self) -> Vec<String> {
        sky::recent(&self.store, 50, self.at())
            .unwrap()
            .into_iter()
            .map(|r| r.label)
            .collect()
    }

    /// Lo que enseña el explorador, carpeta › archivo, bajando por todo.
    fn explorador(&self) -> Vec<String> {
        let mut out = Vec::new();
        let raiz = sky::children(&self.store, None, self.at()).unwrap();
        for r in raiz.rows {
            if r.kind == "folder" {
                let dentro = sky::children(&self.store, Some(&EntityId(r.id)), self.at()).unwrap();
                for d in dentro.rows {
                    out.push(format!("{} › {}", r.label, d.label));
                }
            } else {
                out.push(r.label);
            }
        }
        out.sort();
        out
    }

    fn buscar(&self, q: &str) -> Vec<String> {
        obrain_extract::search(&self.store, q, 10, self.at())
            .unwrap()
            .into_iter()
            .map(|h| h.path.rsplit(['/', '\\']).next().unwrap().to_string())
            .collect()
    }

    fn caduca(&self) -> Vec<String> {
        sky::expiring(
            &self.store,
            self.at(),
            obrain_expiry::Fecha::hoy(obrain_scan::now_ms()),
            10_000,
            false,
        )
        .unwrap()
        .into_iter()
        .map(|e| e.row.label)
        .collect()
    }

    fn papelera(&self) -> Vec<papelera::EnPapelera> {
        papelera::list(&self.store, self.at()).unwrap()
    }

    fn entidad(&self, rel: &str) -> EntityId {
        let ruta = self.ruta(rel).to_string_lossy().to_string();
        self.store
            .find_subject_by_value("path_observed_at", &ruta, self.at())
            .unwrap()
            .unwrap_or_else(|| panic!("nadie vive en {ruta}"))
    }

    fn fragmentos(&self, e: &EntityId) -> i64 {
        self.store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE entity = ?1",
                [e.as_str()],
                |r| r.get(0),
            )
            .unwrap()
    }
}

impl Drop for Boveda {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn mover(de: &Path, a: &Path) {
    std::fs::create_dir_all(a.parent().unwrap()).unwrap();
    std::fs::rename(de, a).unwrap();
}

/// **El caso que lo pidió.** Borras el pasaporte y desaparece de Recientes,
/// de «Caduca pronto», del explorador y de la búsqueda; y aparece en la
/// papelera con su nombre, la carpeta donde estaba y 30 días por delante.
#[test]
fn lo_que_borras_sale_de_todas_partes_y_entra_en_la_papelera() {
    let mut b = Boveda::nueva("borrar");
    let pasaporte = b.entidad("01 Identidad/pasaporte.txt");
    assert!(b.recientes().contains(&"pasaporte.txt".to_string()));
    assert!(b.caduca().contains(&"pasaporte.txt".to_string()));
    assert_eq!(b.buscar("pasaporte"), ["pasaporte.txt"]);

    std::fs::remove_file(b.ruta("01 Identidad/pasaporte.txt")).unwrap();
    let r = b.indexar();
    assert_eq!(r.trashed, 1);

    // Sin orden: «Recientes» pone primero lo último que se modificó, y en
    // NTFS la receta, escrita después, es más reciente; en Linux empatan.
    let mut recientes = b.recientes();
    recientes.sort();
    assert_eq!(recientes, ["poliza.txt", "receta.txt"]);
    assert!(!b.caduca().contains(&"pasaporte.txt".to_string()));
    assert!(b.buscar("pasaporte").is_empty());
    assert_eq!(
        b.explorador(),
        ["02 Salud › poliza.txt", "02 Salud › receta.txt"],
        "y su carpeta, vacía, tampoco sale"
    );

    let p = b.papelera();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].id, pasaporte.0);
    assert_eq!(p[0].label, "pasaporte.txt");
    assert_eq!(p[0].folder.as_deref(), Some("01 Identidad"));
    assert_eq!(p[0].days_left, 30);
    assert!(p[0].path.ends_with("pasaporte.txt"));

    // Su texto sigue ahí para verlo desde la papelera, y el log sabe dónde
    // estuvo: la ruta se retractó, no se borró (I4).
    assert!(b.fragmentos(&pasaporte) > 0);
    let vista = obrain_api::entity::view(&b.store, &pasaporte, b.at()).unwrap();
    assert!(vista.path.is_none());
    assert_eq!(vista.label, "pasaporte.txt");
    assert!(vista.trashed.is_some());

    // Y una pasada más no lo vuelve a mandar a la papelera.
    assert_eq!(b.indexar().trashed, 0);
}

/// Mover un archivo dentro de la bóveda **no** lo deja en su carpeta vieja. Es
/// el fantasma que salía antes de ADR-0025: el mismo documento en dos sitios.
#[test]
fn mover_un_archivo_no_deja_un_fantasma_en_su_carpeta_vieja() {
    let mut b = Boveda::nueva("mover");
    let receta = b.entidad("02 Salud/receta.txt");
    mover(
        &b.ruta("02 Salud/receta.txt"),
        &b.ruta("01 Identidad/receta.txt"),
    );
    let r = b.indexar();
    assert_eq!(r.trashed, 0, "moverlo no es borrarlo");
    assert_eq!(r.files_new, 0, "es el mismo documento");

    assert_eq!(
        b.explorador(),
        [
            "01 Identidad › pasaporte.txt",
            "01 Identidad › receta.txt",
            "02 Salud › poliza.txt",
        ]
    );
    assert_eq!(b.entidad("01 Identidad/receta.txt"), receta);
    let rutas = b
        .store
        .query(Some(&receta), Some("path_observed_at"), b.at())
        .unwrap();
    assert_eq!(rutas.len(), 1, "una sola ruta vigente");
    assert!(b.papelera().is_empty());
}

/// Renombrar una carpeta no deja la vieja con sus documentos al lado de la
/// nueva.
#[test]
fn renombrar_una_carpeta_no_la_duplica() {
    let mut b = Boveda::nueva("renombrar");
    std::fs::rename(b.ruta("02 Salud"), b.ruta("02 Salud y seguros")).unwrap();
    b.indexar();
    assert_eq!(
        b.explorador(),
        [
            "01 Identidad › pasaporte.txt",
            "02 Salud y seguros › poliza.txt",
            "02 Salud y seguros › receta.txt",
        ]
    );
    assert!(b.papelera().is_empty());
}

/// Un USB desconectado, o la carpeta de la biblioteca movida, no es «se borró
/// todo»: no se toca nada.
#[test]
fn una_biblioteca_que_falta_entera_no_manda_nada_a_la_papelera() {
    let mut b = Boveda::nueva("usb");
    let fuera = b.dir.join("en otro sitio");
    std::fs::rename(&b.lib, &fuera).unwrap();
    let r = b.indexar();
    assert_eq!(r.missing.len(), 1);
    assert_eq!(r.trashed, 0);
    assert_eq!(b.recientes().len(), 3, "todo sigue como estaba");
    assert!(b.papelera().is_empty());
    std::fs::rename(&fuera, &b.lib).unwrap();
}

/// Lo sacas de la papelera de Windows —o lo devuelves a la bóveda— con la
/// misma identidad: vuelve con todo, **tu corrección incluida**.
#[test]
fn si_vuelve_vuelve_con_lo_que_sabias() {
    let mut b = Boveda::nueva("vuelve");
    let poliza = b.entidad("02 Salud/poliza.txt");
    obrain_expiry::set_by_user(&b.store, &poliza, None, obrain_scan::now_ms()).unwrap();

    let fuera = b.dir.join("Escritorio/poliza.txt");
    mover(&b.ruta("02 Salud/poliza.txt"), &fuera);
    assert_eq!(b.indexar().trashed, 1);
    assert_eq!(b.papelera().len(), 1);

    mover(&fuera, &b.ruta("02 Salud/poliza.txt"));
    let r = b.indexar();
    assert_eq!(r.returned, 1);
    assert_eq!(r.files_new, 0);
    assert!(b.papelera().is_empty());
    assert_eq!(b.entidad("02 Salud/poliza.txt"), poliza);
    assert!(b.recientes().contains(&"poliza.txt".to_string()));

    // «No caduca», lo que dijiste, sigue mandando.
    let vence = b
        .store
        .resolve(&poliza, obrain_expiry::PREDICATE, b.at())
        .unwrap()
        .unwrap();
    assert_eq!(vence.method, Method::User);
}

/// Copiado de una copia de seguridad —otro archivo, el mismo contenido— sigue
/// siendo el mismo documento.
#[test]
fn si_vuelve_con_el_mismo_contenido_es_el_mismo_documento() {
    let mut b = Boveda::nueva("respaldo");
    let receta = b.entidad("02 Salud/receta.txt");
    let bytes = std::fs::read(b.ruta("02 Salud/receta.txt")).unwrap();
    std::fs::remove_file(b.ruta("02 Salud/receta.txt")).unwrap();
    b.indexar();

    // Otra carpeta y otro nombre: sólo lo delata el contenido.
    std::fs::write(b.ruta("01 Identidad/receta (copia).txt"), &bytes).unwrap();
    let r = b.indexar();
    assert_eq!(r.returned, 1);
    assert_eq!(r.files_new, 0);
    assert_eq!(b.entidad("01 Identidad/receta (copia).txt"), receta);
}

/// «Olvidar ya»: el texto y lo que se indexó de él se van; el log se queda, y
/// la fecha de caducidad no la retracta la pasada siguiente por quedarse sin
/// texto.
#[test]
fn olvidar_borra_lo_leido_y_no_el_log() {
    let mut b = Boveda::nueva("olvidar");
    let pasaporte = b.entidad("01 Identidad/pasaporte.txt");
    std::fs::remove_file(b.ruta("01 Identidad/pasaporte.txt")).unwrap();
    b.indexar();
    let log = b.store.log_len().unwrap();

    let n = b
        .store
        .transaction(|s| papelera::forget(s, Some(&pasaporte), Method::User, obrain_scan::now_ms()))
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(b.fragmentos(&pasaporte), 0);
    assert!(b.papelera().is_empty());
    assert_eq!(
        b.store.log_len().unwrap(),
        log + 1,
        "sólo se añade «olvidado el»: nada se borra del log"
    );

    b.indexar();
    let vence = b
        .store
        .resolve(&pasaporte, obrain_expiry::PREDICATE, b.at())
        .unwrap();
    assert_eq!(
        vence
            .and_then(|a| a.object.as_text().map(str::to_string))
            .as_deref(),
        Some("2031-03-12"),
        "el log sigue sabiendo cuándo caducaba"
    );
}

/// Lo que sigue en tu bóveda no se olvida por error.
#[test]
fn no_se_olvida_lo_que_no_esta_en_la_papelera() {
    let mut b = Boveda::nueva("vivo");
    let poliza = b.entidad("02 Salud/poliza.txt");
    let n = b
        .store
        .transaction(|s| papelera::forget(s, Some(&poliza), Method::User, obrain_scan::now_ms()))
        .unwrap();
    assert_eq!(n, 0);
    assert!(b.fragmentos(&poliza) > 0);
}

/// A los 30 días se olvida solo; «Vaciar papelera» olvida todo ya.
#[test]
fn a_los_30_dias_se_olvida_solo_y_vaciar_olvida_todo() {
    let mut b = Boveda::nueva("plazo");
    std::fs::remove_file(b.ruta("01 Identidad/pasaporte.txt")).unwrap();
    std::fs::remove_file(b.ruta("02 Salud/receta.txt")).unwrap();
    b.indexar();
    assert_eq!(b.papelera().len(), 2);

    let ahora = obrain_scan::now_ms();
    assert_eq!(papelera::expire(&b.store, ahora + 29 * DIA).unwrap(), 0);
    assert_eq!(papelera::expire(&b.store, ahora + 31 * DIA).unwrap(), 2);

    let mut c = Boveda::nueva("vaciar");
    std::fs::remove_file(c.ruta("01 Identidad/pasaporte.txt")).unwrap();
    std::fs::remove_file(c.ruta("02 Salud/receta.txt")).unwrap();
    c.indexar();
    let n = c
        .store
        .transaction(|s| papelera::forget(s, None, Method::User, obrain_scan::now_ms()))
        .unwrap();
    assert_eq!(n, 2);
    assert!(c.papelera().is_empty());
}

/// Olvidado y de vuelta: es el mismo documento, y se vuelve a leer.
#[test]
fn un_olvidado_que_vuelve_se_relee() {
    let mut b = Boveda::nueva("relee");
    let receta = b.entidad("02 Salud/receta.txt");
    let fuera = b.dir.join("receta.txt");
    mover(&b.ruta("02 Salud/receta.txt"), &fuera);
    b.indexar();
    b.store
        .transaction(|s| papelera::forget(s, None, Method::User, obrain_scan::now_ms()))
        .unwrap();
    assert_eq!(b.fragmentos(&receta), 0);

    mover(&fuera, &b.ruta("02 Salud/receta.txt"));
    let r = b.indexar();
    assert_eq!(r.returned, 1);
    assert!(b.fragmentos(&receta) > 0, "se ha vuelto a leer");
    assert_eq!(b.buscar("ibuprofeno"), ["receta.txt"]);
}
