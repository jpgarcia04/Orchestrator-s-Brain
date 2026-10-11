//! La carpeta sigue a su `FileID` ([ADR-0027]): renombrar o mover una carpeta
//! marcada como sensible no la desprotege.
//!
//! Con el pipeline real (`indexar`) sobre carpetas de verdad: en Windows es el
//! `FileID` de NTFS; en Linux, el inodo.
//!
//! [ADR-0027]: ../../../docs/adr/0027-la-carpeta-sigue-a-su-fileid.md

use obrain_api::{indexar, marks, sky};
use obrain_core::{EntityId, Sensitivity, Store, TimePoint};
use std::path::PathBuf;

struct Boveda {
    dir: PathBuf,
    lib: PathBuf,
    store: Store,
}

impl Boveda {
    fn nueva(nombre: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("obrain-renombrar-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lib = dir.join("Boveda");
        std::fs::create_dir_all(lib.join("01 Identidad")).unwrap();
        std::fs::create_dir_all(lib.join("02 Salud")).unwrap();
        // Nombres y textos que el clasificador no ve sensibles: lo que los
        // protege es la carpeta.
        std::fs::write(lib.join("01 Identidad/IMG_2291.txt"), "Foto de la tarjeta.").unwrap();
        std::fs::write(lib.join("02 Salud/receta.txt"), "Receta de ibuprofeno.").unwrap();
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

    fn at(&self) -> TimePoint {
        TimePoint::at(obrain_scan::now_ms())
    }

    /// `rel` con `/`, con el separador del sistema: es como lo guarda el
    /// escáner.
    fn ruta(&self, rel: &str) -> String {
        rel.split('/')
            .fold(self.lib.clone(), |p, t| p.join(t))
            .to_string_lossy()
            .to_string()
    }

    fn carpeta(&self, rel: &str) -> EntityId {
        self.store
            .find_subject_by_value("folder_path", &self.ruta(rel), self.at())
            .unwrap()
            .unwrap_or_else(|| panic!("no hay carpeta {rel}"))
    }

    fn documento(&self, rel: &str) -> EntityId {
        self.store
            .find_subject_by_value("path_observed_at", &self.ruta(rel), self.at())
            .unwrap()
            .unwrap_or_else(|| panic!("nadie vive en {rel}"))
    }

    fn marcar(&self, rel: &str) {
        obrain_guard::mark_folder(
            &self.store,
            &self.carpeta(rel),
            Some(Sensitivity::Sensitive),
            obrain_scan::now_ms(),
        )
        .unwrap();
    }

    fn nivel(&self, rel: &str) -> Sensitivity {
        self.store.sensitivity(&self.documento(rel)).unwrap()
    }

    /// Los nombres de lo que cuelga de una carpeta (o de la raíz).
    fn dentro(&self, carpeta: Option<&EntityId>) -> Vec<String> {
        let mut v: Vec<String> = sky::children(&self.store, carpeta, self.at())
            .unwrap()
            .rows
            .into_iter()
            .map(|r| r.label)
            .collect();
        v.sort();
        v
    }
}

impl Drop for Boveda {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// **El caso que lo pidió** (ADR-0020, «En contra»): renombras `01 Identidad`
/// y lo de dentro sigue sensible, con la misma carpeta y la misma marca.
#[test]
fn renombrar_una_carpeta_marcada_no_la_desprotege() {
    let mut b = Boveda::nueva("marcada");
    let identidad = b.carpeta("01 Identidad");
    b.marcar("01 Identidad");
    assert_eq!(b.nivel("01 Identidad/IMG_2291.txt"), Sensitivity::Sensitive);

    std::fs::rename(
        b.lib.join("01 Identidad"),
        b.lib.join("01 Identidad y viajes"),
    )
    .unwrap();
    let r = b.indexar();
    assert_eq!(r.folders_moved, 1);
    assert_eq!(r.trashed, 0);

    assert_eq!(
        b.carpeta("01 Identidad y viajes"),
        identidad,
        "es la misma carpeta, con otro nombre"
    );
    assert_eq!(
        b.nivel("01 Identidad y viajes/IMG_2291.txt"),
        Sensitivity::Sensitive,
        "y lo de dentro sigue protegido"
    );
    assert_eq!(b.dentro(None), ["01 Identidad y viajes", "02 Salud"]);

    let marcadas = marks::marked(&b.store, b.at()).unwrap();
    assert_eq!(marcadas.len(), 1);
    assert_eq!(marcadas[0].label, "01 Identidad y viajes");
    assert_eq!(marcadas[0].documents, 1, "«Sensibles» ya no la ve vacía");

    // Y otra pasada no cambia nada.
    assert_eq!(b.indexar().folders_moved, 0);
}

/// Moverla dentro de otra carpeta: cuelga sólo de la nueva y sigue marcada.
#[test]
fn mover_una_carpeta_marcada_a_otra_madre() {
    let mut b = Boveda::nueva("mover");
    let identidad = b.carpeta("01 Identidad");
    b.marcar("01 Identidad");

    std::fs::create_dir_all(b.lib.join("Archivo")).unwrap();
    std::fs::write(b.lib.join("Archivo/viejo.txt"), "Notas viejas.").unwrap();
    std::fs::rename(
        b.lib.join("01 Identidad"),
        b.lib.join("Archivo/01 Identidad"),
    )
    .unwrap();
    let r = b.indexar();
    assert_eq!(r.folders_moved, 1);

    assert_eq!(b.carpeta("Archivo/01 Identidad"), identidad);
    assert_eq!(
        b.nivel("Archivo/01 Identidad/IMG_2291.txt"),
        Sensitivity::Sensitive
    );
    assert_eq!(b.dentro(None), ["02 Salud", "Archivo"]);
    assert_eq!(
        b.dentro(Some(&b.carpeta("Archivo"))),
        ["01 Identidad", "viejo.txt"]
    );
}

/// Renombrar la madre muda también a la hija marcada.
#[test]
fn renombrar_la_madre_de_una_carpeta_marcada() {
    let mut b = Boveda::nueva("madre");
    std::fs::create_dir_all(b.lib.join("Papeles")).unwrap();
    std::fs::rename(
        b.lib.join("01 Identidad"),
        b.lib.join("Papeles/01 Identidad"),
    )
    .unwrap();
    b.indexar();
    let identidad = b.carpeta("Papeles/01 Identidad");
    b.marcar("Papeles/01 Identidad");

    std::fs::rename(b.lib.join("Papeles"), b.lib.join("Documentos")).unwrap();
    b.indexar();
    assert_eq!(b.carpeta("Documentos/01 Identidad"), identidad);
    assert_eq!(
        b.nivel("Documentos/01 Identidad/IMG_2291.txt"),
        Sensitivity::Sensitive
    );
}

/// La marca es de la carpeta, no de su nombre: otra carpeta con el nombre
/// viejo nace sin marca, y la marca sigue con la de siempre.
#[test]
fn otra_carpeta_con_el_nombre_viejo_nace_sin_marca() {
    let mut b = Boveda::nueva("nombre-viejo");
    b.marcar("01 Identidad");

    std::fs::rename(b.lib.join("01 Identidad"), b.lib.join("Viejo")).unwrap();
    std::fs::create_dir_all(b.lib.join("01 Identidad")).unwrap();
    std::fs::write(b.lib.join("01 Identidad/nuevo.txt"), "Documento nuevo.").unwrap();
    b.indexar();

    assert_eq!(b.nivel("Viejo/IMG_2291.txt"), Sensitivity::Sensitive);
    assert_eq!(b.nivel("01 Identidad/nuevo.txt"), Sensitivity::Normal);
    let marcadas = marks::marked(&b.store, b.at()).unwrap();
    assert_eq!(marcadas.len(), 1);
    assert_eq!(marcadas[0].label, "Viejo");
}
