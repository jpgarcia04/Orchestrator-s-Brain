#[path = "fixtures/legacy_v1.rs"]
mod legacy;
use legacy::*;
use obrain_api::Brain;
use obrain_core::EntityId;
use std::process::Command;

#[test]
fn cli_v1_recupera_antes_de_mostrar_vencimientos() {
    let legacy = Legacy::new("cli", &[doc("VALIDEZ 15 11 2025.")]);
    let output = Command::new(env!("CARGO_BIN_EXE_obrain"))
        .args(["--db", legacy.path().to_str().unwrap(), "vencimientos"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("expediente.txt"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let brain = Brain::open(legacy.path()).unwrap();
    assert!(
        !brain
            .store()
            .content_pending(&EntityId(DOC.into()))
            .unwrap()
    );
}
