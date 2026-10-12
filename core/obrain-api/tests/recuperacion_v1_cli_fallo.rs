#[path = "fixtures/legacy_v1.rs"]
mod legacy;
use legacy::*;
use obrain_core::{EntityId, Store};
use std::process::Command;

#[test]
fn cli_informa_fallo_de_bootstrap_y_reintenta_sin_resultado_enganoso() {
    let legacy = Legacy::new("cli-fail", &[doc("Documento normal sin fecha.")]);
    let store = Store::open(legacy.path()).unwrap();
    store
        .conn()
        .execute_batch(
            "CREATE TRIGGER abort_expiry_cli
             BEFORE UPDATE OF retracted_at ON assertions
             WHEN OLD.predicate='expires_on' AND OLD.source LIKE 'expiry%'
             BEGIN SELECT RAISE(ABORT,'fallo sintético de bootstrap CLI'); END",
        )
        .unwrap();
    drop(store);

    let run = || {
        Command::new(env!("CARGO_BIN_EXE_obrain"))
            .args(["--db", legacy.path().to_str().unwrap(), "vencimientos"])
            .output()
            .unwrap()
    };
    let first = run();
    assert!(!first.status.success());
    assert!(String::from_utf8_lossy(&first.stderr).contains("no se pudo recuperar"));
    assert!(!String::from_utf8_lossy(&first.stdout).contains("nada caduca"));
    let pending = Store::open(legacy.path()).unwrap();
    assert!(pending.content_pending(&EntityId(DOC.into())).unwrap());
    pending
        .conn()
        .execute_batch("DROP TRIGGER abort_expiry_cli")
        .unwrap();
    drop(pending);

    let retry = run();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert!(String::from_utf8_lossy(&retry.stdout).contains("nada caduca"));
    assert!(
        !Store::open(legacy.path())
            .unwrap()
            .content_pending(&EntityId(DOC.into()))
            .unwrap()
    );
}
