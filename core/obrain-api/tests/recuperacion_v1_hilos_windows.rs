#[path = "fixtures/legacy_v1.rs"]
mod legacy;

use legacy::{Document, Legacy, doc};
use obrain_api::Brain;
use obrain_core::{EntityId, Store};

#[test]
fn dos_bases_v1_en_hilos_sucesivos_y_segunda_conexion() {
    for n in 0..2 {
        let text = if n == 0 {
            "Informe sintético: faro visible tras migrar."
        } else {
            "Segundo informe sintético: el faro sigue visible tras migrar."
        };
        let mut verified = doc(text);
        verified.id = "v1-verificado";
        verified.name = "verificado.txt";
        let quarantined = Document {
            id: "v1-cuarentena",
            name: "cuarentena.pdf",
            text: "No es un PDF válido. secreto-sintetico debe quedar oculto.",
            old_date: None,
            manual_date: None,
            manual_sensitivity: None,
            extraction_verified: false,
        };
        let fixture = Legacy::new(&format!("hilos-{n}"), &[verified, quarantined]);
        fixture.add_current_identities();
        let path = fixture.path().to_path_buf();
        std::thread::spawn(move || {
            eprintln!("v1[{n}]: abrir conexión principal");
            let brain = Brain::try_from_store(Store::open(&path).unwrap()).unwrap();
            eprintln!("v1[{n}]: migración principal terminada");
            let good = EntityId("v1-verificado".into());
            let bad = EntityId("v1-cuarentena".into());
            assert!(!brain.store().content_pending(&good).unwrap());
            assert!(brain.store().content_pending(&bad).unwrap());
            assert!(
                brain
                    .search("faro", 10)
                    .unwrap()
                    .hits
                    .iter()
                    .any(|h| h.entity == good.0)
            );
            assert!(
                brain
                    .search("secreto-sintetico", 10)
                    .unwrap()
                    .hits
                    .is_empty()
            );
            assert!(brain.preview(&bad, None).is_err());

            eprintln!("v1[{n}]: abrir conexión de fondo");
            let mut background = Store::open(&path).unwrap();
            Brain::initialize_store(&mut background).unwrap();
            eprintln!("v1[{n}]: conexión de fondo preparada");
            assert!(background.content_pending(&bad).unwrap());
            let quick: String = background
                .conn()
                .query_row("PRAGMA quick_check", [], |row| row.get(0))
                .unwrap();
            assert_eq!(quick, "ok");
            drop(background);
            drop(brain);

            eprintln!("v1[{n}]: reabrir");
            let reopened = Brain::try_from_store(Store::open(&path).unwrap()).unwrap();
            eprintln!("v1[{n}]: reapertura terminada");
            assert!(!reopened.store().content_pending(&good).unwrap());
            assert!(reopened.store().content_pending(&bad).unwrap());
        })
        .join()
        .unwrap();
    }
}
