//! Comprobación acotada de coherencia de versión en todos los manifiestos del proyecto.

use std::fs;
use std::path::Path;

#[test]
fn version_del_proyecto_es_coherente_en_todos_los_manifiestos() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let version_esperada = "0.0.2";

    // 1. Manifiestos de Cargo del núcleo
    let crates = [
        "core/obrain-api/Cargo.toml",
        "core/obrain-core/Cargo.toml",
        "core/obrain-embed/Cargo.toml",
        "core/obrain-expiry/Cargo.toml",
        "core/obrain-extract/Cargo.toml",
        "core/obrain-guard/Cargo.toml",
        "core/obrain-keys/Cargo.toml",
        "core/obrain-link/Cargo.toml",
        "core/obrain-ocr/Cargo.toml",
        "core/obrain-scan/Cargo.toml",
        "ui/src-tauri/Cargo.toml",
    ];

    for rel in crates {
        let manifest_path = base.join(rel);
        let content = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", manifest_path.display()));
        let has_version = content
            .lines()
            .any(|l| l.trim() == format!("version = \"{version_esperada}\""));
        assert!(
            has_version,
            "El manifiesto {} no declara version = \"{}\"",
            rel, version_esperada
        );
    }

    // 2. Configuración de Tauri
    let tauri_conf_path = base.join("ui/src-tauri/tauri.conf.json");
    let tauri_conf: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&tauri_conf_path)
            .unwrap_or_else(|e| panic!("no se pudo leer tauri.conf.json: {e}")),
    )
    .expect("tauri.conf.json debe ser JSON válido");
    assert_eq!(
        tauri_conf["version"].as_str(),
        Some(version_esperada),
        "tauri.conf.json no declara versión {}",
        version_esperada
    );

    // 3. Manifiestos de Node
    let package_json_path = base.join("ui/package.json");
    let pkg_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&package_json_path)
            .unwrap_or_else(|e| panic!("no se pudo leer ui/package.json: {e}")),
    )
    .expect("ui/package.json debe ser JSON válido");
    assert_eq!(
        pkg_json["version"].as_str(),
        Some(version_esperada),
        "ui/package.json no declara versión {}",
        version_esperada
    );
}
