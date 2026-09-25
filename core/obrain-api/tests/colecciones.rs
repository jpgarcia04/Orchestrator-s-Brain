//! Tus colecciones ([ADR-0031]), de punta a punta: una bóveda de verdad
//! indexada con el pipeline real, colecciones que la cruzan, y lo que les
//! pasa cuando borras, mueves o restauras en el disco.
//!
//! [ADR-0031]: ../../../docs/adr/0031-tus-colecciones.md

use obrain_api::{Brain, colecciones, indexar, sky};
use obrain_core::{EntityId, Method, Sensitivity, Store, TimePoint};
use std::path::PathBuf;

const UI: &str = "user@ui";

/// Un texto de `n` párrafos sobre `tema`, como los de un estado de cuenta.
fn texto(tema: &str, n: usize) -> String {
    (0..n)
        .map(|i| format!("{tema}, movimiento {i}: pago con referencia y su importe del periodo."))
        .collect::<Vec<_>>()
        .join("\n\n")
}

struct Boveda {
    lib: PathBuf,
    store: Store,
}

impl Boveda {
    /// El pasaporte en `01 Identidad`, dos estados de cuenta en `06 Dinero`,
    /// el expediente que junta el de marzo en `04 Trámites` y el contrato
    /// del piso en `05 Vivienda`.
    fn nueva(nombre: &str) -> Self {
        let d = std::env::temp_dir().join(format!(
            "obrain-colecciones-{nombre}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        let lib = d.join("Bóveda");
        let b = Boveda {
            lib,
            store: Store::open_in_memory().unwrap(),
        };
        b.escribir(
            "01 Identidad/Pasaporte.txt",
            "Pasaporte de los Estados Unidos Mexicanos. Nacionalidad mexicana. \
             Fecha de caducidad: 12/03/2031.",
        );
        b.escribir("06 Dinero/Movimientos marzo.txt", &texto("Marzo", 5));
        b.escribir("06 Dinero/Movimientos abril.txt", &texto("Abril", 6));
        b.escribir(
            "04 Trámites/Expediente consulado.txt",
            &format!(
                "{}\n\n{}",
                texto("Portada del expediente", 2),
                texto("Marzo", 5)
            ),
        );
        b.escribir(
            "05 Vivienda/Contrato piso.txt",
            "Contrato de arrendamiento del piso en Málaga. Vence el 30 de septiembre de 2026.",
        );
        let mut b = b;
        b.indexar();
        b
    }

    /// `rel` con `/`: en Windows se arma con `\`, que es lo que guarda el
    /// escáner.
    fn ruta(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.lib.clone(), |p, t| p.join(t))
    }

    fn escribir(&self, rel: &str, texto: &str) {
        let r = self.ruta(rel);
        std::fs::create_dir_all(r.parent().unwrap()).unwrap();
        std::fs::write(r, texto).unwrap();
    }

    fn indexar(&mut self) -> obrain_api::IndexReport {
        indexar::index_with(&mut self.store, std::slice::from_ref(&self.lib), None, None).unwrap()
    }

    fn at(&self) -> TimePoint {
        TimePoint::at(obrain_scan::now_ms())
    }

    /// El documento de esta ruta.
    fn doc(&self, rel: &str) -> EntityId {
        colecciones::documento_en(&self.store, &self.ruta(rel).to_string_lossy(), self.at())
            .unwrap()
            .unwrap_or_else(|| panic!("no está {rel}"))
    }

    /// La carpeta de esta ruta.
    fn carpeta(&self, rel: &str) -> EntityId {
        obrain_api::marks::folder_at(&self.store, &self.ruta(rel).to_string_lossy(), self.at())
            .unwrap()
            .unwrap()
    }

    fn crear(&self, nombre: &str) -> EntityId {
        colecciones::crear(&self.store, nombre, obrain_scan::now_ms(), UI).unwrap()
    }

    fn meter(&self, c: &EntityId, rels: &[&str]) -> usize {
        let docs: Vec<EntityId> = rels.iter().map(|r| self.doc(r)).collect();
        colecciones::meter(&self.store, c, &docs, obrain_scan::now_ms(), UI).unwrap()
    }

    /// Lo que enseña la colección abierta: `nombre · carpeta`, con `↳` si va
    /// debajo de su paquete.
    fn abrir(&self, c: &EntityId) -> Vec<String> {
        colecciones::abrir(&self.store, c, self.at())
            .unwrap()
            .rows
            .into_iter()
            .map(|m| {
                format!(
                    "{}{} · {}",
                    if m.inside.is_some() { "↳ " } else { "" },
                    m.row.label,
                    m.folder_label
                )
            })
            .collect()
    }

    fn todas(&self) -> Vec<(String, usize, usize)> {
        colecciones::todas(&self.store, self.at())
            .unwrap()
            .into_iter()
            .map(|c| (c.name, c.documents, c.in_trash))
            .collect()
    }
}

#[test]
fn una_coleccion_junta_documentos_de_carpetas_distintas() {
    let b = Boveda::nueva("junta");
    let visa = b.crear("Trámite de la visa");
    assert_eq!(
        b.meter(
            &visa,
            &[
                "01 Identidad/Pasaporte.txt",
                "06 Dinero/Movimientos marzo.txt"
            ]
        ),
        2
    );

    // Por nombre, cada uno con su carpeta de verdad.
    assert_eq!(
        b.abrir(&visa),
        vec![
            "Movimientos marzo.txt · 06 Dinero",
            "Pasaporte.txt · 01 Identidad",
        ]
    );
    assert_eq!(b.todas(), vec![("Trámite de la visa".to_string(), 2, 0)]);

    // Meter otra vez lo mismo no cuenta ni escribe nada.
    let antes = b.store.log_len().unwrap();
    assert_eq!(b.meter(&visa, &["01 Identidad/Pasaporte.txt"]), 0);
    assert_eq!(b.store.log_len().unwrap(), antes);

    // Todo lo escribió el usuario (I5): ningún enlazador lo toca.
    let pertenencias = b
        .store
        .pointing_at(&visa, Some(colecciones::MEMBER_OF), b.at())
        .unwrap();
    assert_eq!(pertenencias.len(), 2);
    assert!(pertenencias.iter().all(|a| a.method == Method::User));

    // Y el documento no se ha movido de su carpeta.
    let dinero = b.carpeta("06 Dinero");
    let dentro = sky::children(&b.store, Some(&dinero), b.at()).unwrap();
    assert!(
        dentro
            .rows
            .iter()
            .any(|r| r.label == "Movimientos marzo.txt")
    );
}

#[test]
fn un_documento_puede_estar_en_varias_y_su_vista_previa_lo_dice() {
    let b = Boveda::nueva("varias");
    let visa = b.crear("Trámite de la visa");
    let piso = b.crear("Todo lo del piso");
    b.meter(&visa, &["01 Identidad/Pasaporte.txt"]);
    b.meter(
        &piso,
        &[
            "01 Identidad/Pasaporte.txt",
            "05 Vivienda/Contrato piso.txt",
        ],
    );

    let pasaporte = b.doc("01 Identidad/Pasaporte.txt");
    let brain = Brain::from_store(b.store);
    let v = brain.entity(&pasaporte).unwrap();
    let nombres: Vec<&str> = v.collections.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(nombres, vec!["Todo lo del piso", "Trámite de la visa"]);
    // Las colecciones van en su sitio, no repetidas en «Relacionado».
    assert!(
        !v.observed
            .iter()
            .chain(&v.inferred)
            .any(|c| c.predicate == colecciones::MEMBER_OF)
    );
}

#[test]
fn el_nombre_no_se_repite_ni_queda_vacio_y_se_puede_cambiar() {
    let b = Boveda::nueva("nombres");
    let visa = b.crear("Trámite de la visa");
    let piso = b.crear("Piso");
    let now = obrain_scan::now_ms();

    let repetida = colecciones::crear(&b.store, "  trámite DE la visa ", now, UI);
    assert!(
        matches!(repetida, Err(obrain_api::Error::Coleccion(colecciones::Error::Repetida(ref n))) if n == "Trámite de la visa"),
        "{repetida:?}"
    );
    assert!(matches!(
        colecciones::crear(&b.store, "   ", now, UI),
        Err(obrain_api::Error::Coleccion(colecciones::Error::SinNombre))
    ));
    assert!(matches!(
        colecciones::renombrar(&b.store, &piso, "trámite de la visa", now, UI),
        Err(obrain_api::Error::Coleccion(colecciones::Error::Repetida(
            _
        )))
    ));

    // Renombrar: el nombre nuevo manda, el viejo se retracta (no se borra).
    colecciones::renombrar(&b.store, &piso, "Todo lo del piso", now + 1, UI).unwrap();
    assert_eq!(
        obrain_link::label(&b.store, &piso, TimePoint::at(now + 2)),
        "Todo lo del piso"
    );
    let nombres = b
        .store
        .query(Some(&piso), Some(colecciones::NAME), TimePoint::at(now + 2))
        .unwrap();
    assert_eq!(nombres.len(), 1, "sólo un nombre vivo");
    // Cambiar a su propio nombre con otras mayúsculas no es repetirse.
    colecciones::renombrar(&b.store, &visa, "Trámite de la Visa", now + 3, UI).unwrap();
    assert_eq!(
        colecciones::por_nombre(&b.store, "trámite de la visa", TimePoint::at(now + 4)).unwrap(),
        Some(visa)
    );
}

#[test]
fn solo_entran_documentos_y_nada_entra_a_medias() {
    let b = Boveda::nueva("solo");
    let visa = b.crear("Trámite de la visa");
    let pasaporte = b.doc("01 Identidad/Pasaporte.txt");
    let identidad = b.carpeta("01 Identidad");
    let otra = b.crear("Otra");

    for intruso in [&identidad, &otra] {
        let r = colecciones::meter(
            &b.store,
            &visa,
            &[pasaporte.clone(), intruso.clone()],
            obrain_scan::now_ms(),
            UI,
        );
        assert!(
            matches!(
                r,
                Err(obrain_api::Error::Coleccion(
                    colecciones::Error::NoEsDocumento(_)
                ))
            ),
            "{r:?}"
        );
    }
    assert!(
        b.abrir(&visa).is_empty(),
        "si uno no vale, no entra ninguno"
    );
}

#[test]
fn sacar_y_borrar_no_tocan_tus_documentos_y_el_log_lo_recuerda() {
    let b = Boveda::nueva("borrar");
    let visa = b.crear("Trámite de la visa");
    let piso = b.crear("Todo lo del piso");
    b.meter(
        &visa,
        &[
            "01 Identidad/Pasaporte.txt",
            "06 Dinero/Movimientos marzo.txt",
        ],
    );
    b.meter(&piso, &["01 Identidad/Pasaporte.txt"]);
    let antes = b.at();

    let marzo = b.doc("06 Dinero/Movimientos marzo.txt");
    assert_eq!(
        colecciones::sacar(
            &b.store,
            &visa,
            std::slice::from_ref(&marzo),
            obrain_scan::now_ms() + 1
        )
        .unwrap(),
        1
    );
    let despues = TimePoint::at(obrain_scan::now_ms() + 2);
    assert_eq!(
        colecciones::abrir(&b.store, &visa, despues)
            .unwrap()
            .rows
            .len(),
        1
    );

    assert_eq!(
        colecciones::borrar(&b.store, &visa, obrain_scan::now_ms() + 3).unwrap(),
        1
    );
    let luego = TimePoint::at(obrain_scan::now_ms() + 4);
    let quedan: Vec<String> = colecciones::todas(&b.store, luego)
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(quedan, vec!["Todo lo del piso"], "las otras no se tocan");
    let pasaporte = b.doc("01 Identidad/Pasaporte.txt");
    let suyas: Vec<String> = colecciones::de(&b.store, &pasaporte, luego)
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(suyas, vec!["Todo lo del piso"]);
    assert!(matches!(
        colecciones::abrir(&b.store, &visa, luego),
        Err(obrain_api::Error::Coleccion(colecciones::Error::NoExiste))
    ));

    // Tus archivos siguen ahí, y en su carpeta.
    assert!(b.ruta("06 Dinero/Movimientos marzo.txt").exists());
    assert!(b.ruta("01 Identidad/Pasaporte.txt").exists());
    let dinero = sky::children(&b.store, Some(&b.carpeta("06 Dinero")), luego).unwrap();
    assert_eq!(dinero.files, 2);

    // I4: lo que había antes de borrarla se puede seguir preguntando.
    let entonces = colecciones::abrir(&b.store, &visa, antes).unwrap();
    assert_eq!(entonces.name, "Trámite de la visa");
    assert_eq!(entonces.rows.len(), 2);
    // Y su nombre se sigue sabiendo: así se llamaba.
    assert_eq!(
        obrain_link::label(&b.store, &visa, luego),
        "Trámite de la visa"
    );
}

#[test]
fn la_papelera_la_saca_de_la_coleccion_y_restaurarla_la_devuelve() {
    let mut b = Boveda::nueva("papelera");
    let visa = b.crear("Trámite de la visa");
    b.meter(
        &visa,
        &[
            "01 Identidad/Pasaporte.txt",
            "06 Dinero/Movimientos abril.txt",
        ],
    );
    let abril = b.doc("06 Dinero/Movimientos abril.txt");

    let original = std::fs::read(b.ruta("06 Dinero/Movimientos abril.txt")).unwrap();
    std::fs::remove_file(b.ruta("06 Dinero/Movimientos abril.txt")).unwrap();
    b.indexar();

    assert_eq!(b.abrir(&visa), vec!["Pasaporte.txt · 01 Identidad"]);
    let abierta = colecciones::abrir(&b.store, &visa, b.at()).unwrap();
    assert_eq!(abierta.in_trash.len(), 1);
    assert_eq!(abierta.in_trash[0].id, abril.0);
    assert_eq!(b.todas(), vec![("Trámite de la visa".to_string(), 1, 1)]);
    // Lo que está en la papelera no se mete.
    let r = colecciones::meter(
        &b.store,
        &visa,
        std::slice::from_ref(&abril),
        obrain_scan::now_ms(),
        UI,
    );
    assert!(matches!(
        r,
        Err(obrain_api::Error::Coleccion(
            colecciones::Error::NoEsDocumento(_)
        ))
    ));

    // Vuelve (lo sacas de la Papelera de Windows, o el mismo contenido): a su colección.
    std::fs::write(b.ruta("06 Dinero/Movimientos abril.txt"), original).unwrap();
    b.indexar();
    assert_eq!(
        b.doc("06 Dinero/Movimientos abril.txt"),
        abril,
        "es el mismo"
    );
    assert_eq!(
        b.abrir(&visa),
        vec![
            "Movimientos abril.txt · 06 Dinero",
            "Pasaporte.txt · 01 Identidad",
        ]
    );
    assert_eq!(b.todas(), vec![("Trámite de la visa".to_string(), 2, 0)]);
}

#[test]
fn mover_o_renombrar_un_documento_no_lo_saca_de_su_coleccion() {
    let mut b = Boveda::nueva("mover");
    let piso = b.crear("Todo lo del piso");
    b.meter(&piso, &["05 Vivienda/Contrato piso.txt"]);
    let contrato = b.doc("05 Vivienda/Contrato piso.txt");

    std::fs::create_dir_all(b.ruta("04 Trámites")).unwrap();
    std::fs::rename(
        b.ruta("05 Vivienda/Contrato piso.txt"),
        b.ruta("04 Trámites/Contrato de arrendamiento.txt"),
    )
    .unwrap();
    b.indexar();

    assert_eq!(b.doc("04 Trámites/Contrato de arrendamiento.txt"), contrato);
    assert_eq!(
        b.abrir(&piso),
        vec!["Contrato de arrendamiento.txt · 04 Trámites"]
    );
}

#[test]
fn un_paquete_no_mete_lo_que_lleva_y_lo_de_dentro_va_debajo_de_el() {
    let b = Boveda::nueva("paquete");
    let visa = b.crear("Trámite de la visa");
    b.meter(&visa, &["04 Trámites/Expediente consulado.txt"]);
    assert_eq!(
        b.abrir(&visa),
        vec!["Expediente consulado.txt · 04 Trámites"],
        "el de marzo está dentro del expediente, pero no lo metiste tú"
    );

    b.meter(
        &visa,
        &[
            "06 Dinero/Movimientos marzo.txt",
            "01 Identidad/Pasaporte.txt",
        ],
    );
    // Por nombre sería Expediente, Movimientos, Pasaporte; el de marzo va
    // debajo del paquete que lo lleva, un poco metido.
    assert_eq!(
        b.abrir(&visa),
        vec![
            "Expediente consulado.txt · 04 Trámites",
            "↳ Movimientos marzo.txt · 06 Dinero",
            "Pasaporte.txt · 01 Identidad",
        ]
    );
}

#[test]
fn una_coleccion_no_cambia_la_sensibilidad_de_nada() {
    let mut b = Boveda::nueva("sensible");
    let identidad = b.carpeta("01 Identidad");
    obrain_guard::mark_folder(
        &b.store,
        &identidad,
        Some(Sensitivity::Sensitive),
        obrain_scan::now_ms(),
    )
    .unwrap();
    b.indexar();
    let pasaporte = b.doc("01 Identidad/Pasaporte.txt");
    let contrato = b.doc("05 Vivienda/Contrato piso.txt");
    let antes = (
        b.store.sensitivity(&pasaporte).unwrap(),
        b.store.sensitivity(&contrato).unwrap(),
    );
    assert_eq!(antes.0, Sensitivity::Sensitive);

    let todo = b.crear("Todo junto");
    b.meter(
        &todo,
        &[
            "01 Identidad/Pasaporte.txt",
            "05 Vivienda/Contrato piso.txt",
        ],
    );
    b.indexar();
    assert_eq!(
        (
            b.store.sensitivity(&pasaporte).unwrap(),
            b.store.sensitivity(&contrato).unwrap(),
        ),
        antes,
        "juntar no destapa ni protege"
    );
    assert_eq!(b.store.sensitivity(&todo).unwrap(), Sensitivity::Normal);
    // En la colección, cada uno dice lo suyo.
    let filas = colecciones::abrir(&b.store, &todo, b.at()).unwrap().rows;
    let niveles: Vec<(&str, &str)> = filas
        .iter()
        .map(|m| (m.row.label.as_str(), m.row.sensitivity))
        .collect();
    assert_eq!(
        niveles,
        vec![
            ("Contrato piso.txt", antes.1.as_str()),
            ("Pasaporte.txt", "sensitive")
        ]
    );
}

#[test]
fn buscar_dentro_de_una_coleccion_no_pierde_lo_que_estaba_mas_abajo() {
    let mut b = Boveda::nueva("buscar");
    // Veinte documentos que dicen «movimiento» mucho más que el pasaporte, que
    // sólo lo dice una vez: buscando en todo, el pasaporte no sale en los 5
    // primeros.
    for i in 0..20 {
        b.escribir(
            &format!("07 Ruido/Extracto {i:02}.txt"),
            &"Movimiento, movimiento y otro movimiento del periodo. ".repeat(4),
        );
    }
    b.escribir(
        "01 Identidad/Pasaporte.txt",
        "Pasaporte mexicano. Último movimiento migratorio: entrada por Barajas.",
    );
    b.indexar();
    let visa = b.crear("Trámite de la visa");
    b.meter(
        &visa,
        &[
            "01 Identidad/Pasaporte.txt",
            "05 Vivienda/Contrato piso.txt",
        ],
    );

    let brain = Brain::from_store(b.store);
    let en_todo: Vec<String> = brain
        .search("movimiento", 5)
        .unwrap()
        .hits
        .into_iter()
        .map(|h| h.label)
        .collect();
    assert!(
        !en_todo.contains(&"Pasaporte.txt".to_string()),
        "{en_todo:?}"
    );

    let dentro: Vec<String> = brain
        .search_in("movimiento", 5, Some(&visa))
        .unwrap()
        .hits
        .into_iter()
        .map(|h| h.label)
        .collect();
    assert_eq!(dentro, vec!["Pasaporte.txt"]);

    // Y lo que no coincide no sale aunque esté en la colección.
    assert!(
        brain
            .search_in("arrendamiento", 5, Some(&visa))
            .unwrap()
            .hits
            .iter()
            .all(|h| h.label == "Contrato piso.txt")
    );
    // Una colección que no existe es un error, no «buscar en todo».
    assert!(
        brain
            .search_in("movimiento", 5, Some(&EntityId("no-existe".into())))
            .is_err()
    );
}

#[test]
fn la_lente_de_un_documento_ve_su_coleccion_y_lo_que_juntaste_con_el() {
    let b = Boveda::nueva("lente");
    let visa = b.crear("Trámite de la visa");
    b.meter(
        &visa,
        &[
            "01 Identidad/Pasaporte.txt",
            "05 Vivienda/Contrato piso.txt",
        ],
    );
    let pasaporte = b.doc("01 Identidad/Pasaporte.txt");
    let contrato = b.doc("05 Vivienda/Contrato piso.txt");

    let brain = Brain::from_store(b.store);
    let l = brain
        .lens(std::slice::from_ref(&pasaporte), 2, 300)
        .unwrap();
    let nodo = l
        .nodes
        .iter()
        .find(|n| n.id == visa.0)
        .expect("su colección");
    assert_eq!(nodo.label, "Trámite de la visa");
    assert_eq!(nodo.facets, vec!["region"]);
    assert_eq!(nodo.distance, 1);
    assert!(
        l.nodes
            .iter()
            .any(|n| n.id == contrato.0 && n.distance == 2),
        "a dos saltos, lo que juntaste con él"
    );
}

#[test]
fn cada_documento_dice_que_estrella_encender() {
    let b = Boveda::nueva("estrellas");
    let visa = b.crear("Trámite de la visa");
    b.meter(&visa, &["06 Dinero/Movimientos marzo.txt"]);
    let abierta = colecciones::abrir(&b.store, &visa, b.at()).unwrap();
    let m = &abierta.rows[0];
    let dinero = b.carpeta("06 Dinero");
    assert_eq!(
        m.ancestors.first(),
        Some(&dinero.0),
        "de dentro hacia fuera"
    );
    assert_eq!(m.folder, dinero.0);
    assert_eq!(
        m.ancestors.len(),
        2,
        "su carpeta y la bóveda: {:?}",
        m.ancestors
    );
    assert_eq!(abierta.folders, 1);
}

/// Lo que enseña la colección abierta desde la aplicación: `nombre · carpeta`.
fn en(brain: &Brain, c: &EntityId) -> Vec<String> {
    brain
        .collection(c)
        .unwrap()
        .rows
        .into_iter()
        .map(|m| format!("{} · {}", m.row.label, m.folder_label))
        .collect()
}

#[test]
fn mover_de_una_coleccion_a_otra_la_saca_de_la_primera_y_no_toca_tu_carpeta() {
    let b = Boveda::nueva("arrastrar");
    let visa = b.crear("Trámite de la visa");
    let piso = b.crear("Todo lo del piso");
    b.meter(
        &visa,
        &[
            "01 Identidad/Pasaporte.txt",
            "06 Dinero/Movimientos marzo.txt",
        ],
    );
    b.meter(&piso, &["05 Vivienda/Contrato piso.txt"]);
    let marzo = b.doc("06 Dinero/Movimientos marzo.txt");
    let pasaporte = b.doc("01 Identidad/Pasaporte.txt");
    let dinero = b.carpeta("06 Dinero");
    let mut brain = Brain::from_store(b.store);
    let antes = TimePoint::at(obrain_scan::now_ms());
    std::thread::sleep(std::time::Duration::from_millis(5));

    let r = brain
        .move_between_collections(&visa, &piso, std::slice::from_ref(&marzo), UI)
        .unwrap();
    assert_eq!((r.added, r.removed), (1, 1));
    assert_eq!(en(&brain, &visa), vec!["Pasaporte.txt · 01 Identidad"]);
    assert_eq!(
        en(&brain, &piso),
        vec![
            "Contrato piso.txt · 05 Vivienda",
            "Movimientos marzo.txt · 06 Dinero",
        ]
    );

    // En tu carpeta no cambia nada: ni el archivo ni el árbol.
    assert!(
        b.lib
            .join("06 Dinero")
            .join("Movimientos marzo.txt")
            .exists()
    );
    let ahora = TimePoint::at(obrain_scan::now_ms());
    let dentro = sky::children(brain.store(), Some(&dinero), ahora).unwrap();
    assert!(
        dentro
            .rows
            .iter()
            .any(|r| r.label == "Movimientos marzo.txt")
    );

    // Las dos cosas son del usuario (I5), y el log sabe dónde estaba (I4).
    assert!(
        brain
            .store()
            .pointing_at(&piso, Some(colecciones::MEMBER_OF), ahora)
            .unwrap()
            .iter()
            .all(|a| a.method == Method::User)
    );
    let entonces = colecciones::abrir(brain.store(), &visa, antes).unwrap();
    assert_eq!(
        entonces.rows.len(),
        2,
        "antes de moverlo, estaba en la visa"
    );

    // Si ya estaba en la de destino, no entra otra vez, pero sí sale de la
    // de origen: al final está sólo en una.
    let otra = colecciones::meter(
        brain.store(),
        &piso,
        std::slice::from_ref(&pasaporte),
        obrain_scan::now_ms(),
        UI,
    )
    .unwrap();
    assert_eq!(otra, 1);
    let r = brain
        .move_between_collections(&visa, &piso, std::slice::from_ref(&pasaporte), UI)
        .unwrap();
    assert_eq!((r.added, r.removed), (0, 1));
    assert!(en(&brain, &visa).is_empty());
    assert_eq!(en(&brain, &piso).len(), 3);
}

#[test]
fn mover_a_la_misma_no_hace_nada_y_si_algo_no_vale_no_cambia_nada() {
    let mut b = Boveda::nueva("arrastrar-mal");
    let visa = b.crear("Trámite de la visa");
    let piso = b.crear("Todo lo del piso");
    b.meter(
        &visa,
        &[
            "06 Dinero/Movimientos marzo.txt",
            "06 Dinero/Movimientos abril.txt",
        ],
    );
    let marzo = b.doc("06 Dinero/Movimientos marzo.txt");
    let abril = b.doc("06 Dinero/Movimientos abril.txt");
    // Abril se va a la papelera a mitad del arrastre.
    std::fs::remove_file(b.ruta("06 Dinero/Movimientos abril.txt")).unwrap();
    b.indexar();
    let borrada = b.crear("Borrada");
    colecciones::borrar(&b.store, &borrada, obrain_scan::now_ms()).unwrap();
    let mut brain = Brain::from_store(b.store);
    let antes = brain.store().log_len().unwrap();

    // A la misma colección: nada, y no escribe nada.
    let r = brain
        .move_between_collections(&visa, &visa, std::slice::from_ref(&marzo), UI)
        .unwrap();
    assert_eq!((r.added, r.removed), (0, 0));
    assert_eq!(brain.store().log_len().unwrap(), antes);

    // A una colección que ya no existe: error, y marzo sigue en la visa.
    let r = brain.move_between_collections(&visa, &borrada, std::slice::from_ref(&marzo), UI);
    assert!(
        matches!(
            r,
            Err(obrain_api::Error::Coleccion(colecciones::Error::NoExiste))
        ),
        "{r:?}"
    );

    // Con uno que ya está en la papelera: error, y no se mueve ninguno.
    let r = brain.move_between_collections(&visa, &piso, &[marzo.clone(), abril.clone()], UI);
    assert!(
        matches!(
            r,
            Err(obrain_api::Error::Coleccion(
                colecciones::Error::NoEsDocumento(_)
            ))
        ),
        "{r:?}"
    );
    assert_eq!(brain.store().log_len().unwrap(), antes);
    assert_eq!(en(&brain, &visa), vec!["Movimientos marzo.txt · 06 Dinero"]);
    assert!(en(&brain, &piso).is_empty());
}
