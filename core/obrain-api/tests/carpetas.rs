//! Sensibilidad por carpeta ([ADR-0020]) vista desde la superficie: lo que
//! enseñan la cabecera de una carpeta, la vista previa y la vista «Sensibles».
//!
//! El árbol lo monta el enlazador real de M3, igual que en `explorador.rs`.
//!
//! [ADR-0020]: ../../../docs/adr/0020-sensibilidad-por-carpeta.md

use obrain_api::Brain;
use obrain_core::{EntityId, Method, NewAssertion, Sensitivity, Store, TimePoint};
use obrain_link::{FileView, LinkReport};

const T: i64 = 1_700_000_000_000;

fn cerebro(paths: &[&str]) -> (Brain, Vec<EntityId>) {
    let store = Store::open_in_memory().unwrap();
    obrain_extract::index::migrate(store.conn()).unwrap();
    obrain_embed::index::migrate(store.conn()).unwrap();
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
    obrain_link::structure::link(&store, &files, T, &mut r).unwrap();
    let ids = files.into_iter().map(|f| f.entity).collect();
    (Brain::from_store(store), ids)
}

fn carpeta(b: &Brain, ruta: &str) -> EntityId {
    b.store()
        .find_subject_by_value("folder_path", ruta, TimePoint::at(T + 1))
        .unwrap()
        .unwrap_or_else(|| panic!("no hay carpeta {ruta}"))
}

const BOVEDA: &[&str] = &[
    r"C:\Bóveda\01 Identidad\IMG_2291.jpg",
    r"C:\Bóveda\01 Identidad\2024\dni.png",
    r"C:\Bóveda\02 Salud\receta.pdf",
];

/// El botón de la cabecera: marcar sube lo de dentro, y la carpeta lo dice.
#[test]
fn marcar_desde_la_cabecera_protege_lo_de_dentro() {
    let (mut b, ids) = cerebro(BOVEDA);
    let identidad = carpeta(&b, r"C:\Bóveda\01 Identidad");

    assert!(b.children(Some(&identidad)).unwrap().protection.is_none());

    let r = b
        .mark_folder(&identidad, Some(Sensitivity::Sensitive))
        .unwrap();
    assert_eq!(r.level, Some("sensitive"));
    assert_eq!(r.under, 4, "la carpeta, su subcarpeta y sus dos archivos");

    let l = b.children(Some(&identidad)).unwrap();
    let p = l
        .protection
        .expect("la cabecera tiene que saber que está marcada");
    assert!(p.own);
    assert_eq!(p.label, "01 Identidad");
    assert_eq!(p.level, "sensitive");

    // La subcarpeta sale con candado en la lista y, dentro, dice de quién hereda.
    let sub = l.rows.iter().find(|r| r.kind == "folder").unwrap();
    assert_eq!(sub.sensitivity, "sensitive");
    let dentro = b.children(Some(&EntityId(sub.id.clone()))).unwrap();
    let heredada = dentro.protection.expect("la subcarpeta hereda");
    assert!(!heredada.own);
    assert_eq!(heredada.folder, identidad.0);

    assert_eq!(
        b.store().sensitivity(&ids[0]).unwrap(),
        Sensitivity::Sensitive
    );
    assert_eq!(
        b.store().sensitivity(&ids[1]).unwrap(),
        Sensitivity::Sensitive
    );
    assert_eq!(b.store().sensitivity(&ids[2]).unwrap(), Sensitivity::Normal);
}

/// El «¿por qué?» de la vista previa: la foto es sensible por su carpeta.
#[test]
fn la_vista_previa_dice_que_carpeta_lo_protege() {
    let (mut b, ids) = cerebro(BOVEDA);
    let identidad = carpeta(&b, r"C:\Bóveda\01 Identidad");
    assert!(b.entity(&ids[0]).unwrap().protection.is_none());

    b.mark_folder(&identidad, Some(Sensitivity::Sensitive))
        .unwrap();
    let v = b.entity(&ids[0]).unwrap();
    assert_eq!(v.sensitivity, "sensitive");
    let p = v.protection.expect("tiene que decir por qué");
    assert_eq!(p.folder, identidad.0);
    assert_eq!(p.label, "01 Identidad");
    assert!(b.entity(&ids[2]).unwrap().protection.is_none());
}

/// La vista «Sensibles»: qué carpetas marcaste y cuántos documentos protegen.
/// Y lo heredado aparece en la lista de sensibles como cualquier otro.
#[test]
fn sensibles_lista_las_carpetas_marcadas() {
    let (mut b, _) = cerebro(BOVEDA);
    assert!(b.marked_folders().unwrap().is_empty());
    let identidad = carpeta(&b, r"C:\Bóveda\01 Identidad");
    b.mark_folder(&identidad, Some(Sensitivity::Sensitive))
        .unwrap();

    let m = b.marked_folders().unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].label, "01 Identidad");
    assert_eq!(m[0].documents, 2);
    assert_eq!(b.sensitive().unwrap().len(), 2);

    // Quitarla la saca de la lista y devuelve los archivos a normal.
    let r = b.mark_folder(&identidad, None).unwrap();
    assert_eq!(r.level, None);
    assert!(b.marked_folders().unwrap().is_empty());
    assert!(b.sensitive().unwrap().is_empty());
}

/// Un archivo no es una carpeta: el botón no puede marcarlo.
#[test]
fn un_archivo_no_se_marca_como_carpeta() {
    let (mut b, ids) = cerebro(BOVEDA);
    assert!(
        b.mark_folder(&ids[0], Some(Sensitivity::Sensitive))
            .is_err()
    );
}

/// En la raíz no hay cabecera de carpeta que enseñar… salvo que la raíz sea tu
/// bóveda, que es una carpeta como cualquier otra y también se puede marcar.
#[test]
fn la_raiz_tambien_se_puede_marcar() {
    let (mut b, ids) = cerebro(BOVEDA);
    let raiz = b.children(None).unwrap();
    let foco = EntityId(
        raiz.focus
            .clone()
            .expect("con una sola raíz se entra en ella"),
    );
    b.mark_folder(&foco, Some(Sensitivity::Sensitive)).unwrap();
    assert!(b.children(None).unwrap().protection.is_some_and(|p| p.own));
    assert!(
        ids.iter()
            .all(|e| b.store().sensitivity(e).unwrap() == Sensitivity::Sensitive)
    );
}
