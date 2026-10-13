//! Genera una base v1 sintética persistente para probar el ejecutable Tauri.
//! Ejecutar con TEMP y TMP apuntando a un directorio de prueba aislado.

#[path = "../tests/fixtures/legacy_v1.rs"]
mod legacy;

use legacy::{Document, Legacy, doc};

fn main() {
    let mut verified = doc("Informe sintético de prueba. La palabra faro debe ser consultable.");
    verified.id = "v1-verificado";
    verified.name = "verificado.txt";
    verified.old_date = Some("2025-11-15");

    let quarantined = Document {
        id: "v1-cuarentena",
        name: "cuarentena.pdf",
        text: "Esto no es un PDF válido. El texto secreto-sintetico no debe publicarse.",
        old_date: Some("2025-11-15"),
        manual_date: None,
        manual_sensitivity: None,
        extraction_verified: false,
    };

    let legacy = Legacy::new("tauri-windows", &[verified, quarantined]);
    legacy.add_current_identities();
    println!("{}", legacy.path().display());
    std::mem::forget(legacy);
}
