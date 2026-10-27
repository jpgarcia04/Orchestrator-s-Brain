# Candidato Windows 0.0.2 — Orchestrator's Brain

Fecha de actualización: 27 de octubre de 2026.  
Base: `origin/main` en `ccd5251af99594c61b89c0f42dfea071ade22399` (PR #15 fusionada).  
Rama: `release/candidato-windows-0.0.2` (PR #16).

## 1. Resumen ejecutivo

1. **Coherencia de versión `0.0.2`:** Actualizada y verificada en los 10 crates de `core/`, `core/Cargo.lock`, `ui/src-tauri/Cargo.toml`, `ui/src-tauri/Cargo.lock`, `ui/src-tauri/tauri.conf.json`, `ui/package.json` y `ui/package-lock.json`.
2. **Validación en runner Windows desechable (`windows-2022`):**
   - **Ejecución [40146425180](https://github.com/jpgarcia04/Orchestrator-s-Brain/actions/runs/40146425180):** Completó con éxito la compilación, inspección COM estática, instalación limpia (`msiexec /i`, código `0`, `ProductState = 5`), arranque vivo durante 20 segundos (`sessionId = 2`) y desinstalación limpia (`msiexec /x`, código `0`, `ProductState = -1`) del candidato `0.0.2`.
   - **Ejecución [40551632816](https://github.com/jpgarcia04/Orchestrator-s-Brain/actions/runs/40551632816):** Completó la fase de instalación limpia de `0.0.2` y ejecutó la **actualización real desde el MSI histórico `0.0.1`** (`SHA-256: 9AEE1B8125CC19DDBBF124350CD86EB510E77A9313C89094E0797A6AF7E35591`, `ProductCode: {E826D686-D784-4205-95C2-F19231569F02}`) sobre una base sintética generada con la **CLI histórica auténtica `0.0.1`** compilada desde el commit `5421ceeeb977944edec8406119f504f8c8e0a17a`. Acreditó:
     - Instalación de `0.0.1` (`exitCode = 0`, `ProductState = 5`, `obrain-app.exe` `SHA-256: C202C2DF9F8F54E7AAB38C2C83BF4C947004CB2D5DE4289C2FD015B125A113F1`) y arranque vivo durante 10 s sobre el perfil sintético `0.0.1`.
     - Detección por Windows Installer de la versión previa (`FindRelatedProducts` -> `WIX_UPGRADE_DETECTED = {E826D686-D784-4205-95C2-F19231569F02}`) y ejecución de `RemoveExistingProducts` (`Return value 1`).
     - Sustitución completa de producto y registro (`ProductState` `0.0.1` -> `-1`; `ProductState` `0.0.2` -> `5`; `DisplayVersion = 0.0.2`; actualización de `obrain-app.exe` a `SHA-256: 5AD73096306C273D663B2C6E45F96A9DFF9206A6617C9E80E1AF51DBAD75F6C1`).
     - Apertura y supervivencia durante 20 s de `obrain-app.exe` `0.0.2` sobre el perfil sintético creado por `0.0.1` (`aliveAfter20s = true`, `isolatedProfileProtected = true`).
     - Desinstalación limpia de `0.0.2` (`Removal completed successfully`, `MainEngineThread is returning 0`).
3. **Verificación de conservación lógica e integridad SQLCipher (`5421cee` `0.0.1` -> `0.0.2`):**
   - En la ejecución `40551632816`, tras completar el arranque de `0.0.2`, el script PowerShell se detuvo en la línea de lectura de `obrain bibliotecas` por evaluar `.Count` sobre un escalar `[bool]` bajo `Set-StrictMode -Version Latest` (`($libsPost -match ...).Count`).
   - Se corrigió el script [`tools/validar-msi-runner.ps1`](tools/validar-msi-runner.ps1) (`@(@($var) -match ...).Count`) y se ejecutó en un entorno aislado bajo `%TEMP%` la secuencia exacta de base `0.0.1` (creada con `obrain.exe` compilado desde `5421cee`) abierta y verificada por `obrain.exe` `0.0.2`, confirmando la conservación íntegra de bibliotecas, paquetes (`part_of`), carpeta sensible, colección y documentos asociados, así como la verificación criptográfica y estructural mediante `obrain respaldar` y `obrain respaldo --verificar` (`PRAGMA quick_check`, `PRAGMA foreign_key_check`, `PRAGMA cipher_integrity_check` y `PRAGMA integrity_check`).
4. **Política permanente de eficiencia en CI:** Se reestructuraron los workflows ([`.github/workflows/validar-msi-windows.yml`](.github/workflows/validar-msi-windows.yml), [`.github/workflows/verificar-pr.yml`](.github/workflows/verificar-pr.yml), [`tools/verificar-local.ps1`](tools/verificar-local.ps1) y [`docs/POLITICA-CI.md`](docs/POLITICA-CI.md)) para eliminar las ejecuciones automáticas de Windows MSI en cada PR y reducir drásticamente el consumo de minutos facturables.

---

## 2. Identificadores, hashes y artefactos verificados

| Elemento | Versión | Tamaño (bytes) | SHA-256 | ProductCode / Commit | UpgradeCode |
|---|---|---|---|---|---|
| MSI histórico `0.0.1` (referencia) | `0.0.1` | `26.234.880` | `9AEE1B8125CC19DDBBF124350CD86EB510E77A9313C89094E0797A6AF7E35591` | `{E826D686-D784-4205-95C2-F19231569F02}` | `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}` |
| `obrain-app.exe` instalado por MSI `0.0.1` | `0.0.1` | `46.060.544` | `C202C2DF9F8F54E7AAB38C2C83BF4C947004CB2D5DE4289C2FD015B125A113F1` | Base `5421cee` / `c6c57cf` | — |
| CLI histórica `obrain.exe` (runner `40551632816`) | `0.0.1` | — | `F9F3B8363BEEF4075208DAC4571842AD2A8DBA006346D3F8D7468EF9F1ADBFE3` | Commit `5421ceeeb977944edec8406119f504f8c8e0a17a` | — |
| MSI candidato `0.0.2` (runner `40551632816`) | `0.0.2` | `26.128.384` | `05BCC64D8AF18758D538DAAA8080EDADB794EFEEE0169CD93CE04966AEE0BB7E` | `{64052E5F-0B35-4D4A-949B-603A66556284}` | `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}` |
| `obrain-app.exe` instalado por MSI `0.0.2` (runner) | `0.0.2` | `45.386.240` | `5AD73096306C273D663B2C6E45F96A9DFF9206A6617C9E80E1AF51DBAD75F6C1` | Commit `6199060` (merge `e3c684a`) | — |
| CLI candidata `obrain.exe` (runner `40551632816`) | `0.0.2` | — | `1642823181173F98945DEF3966F45109AB35F26B1343262A938670FEE7D49549` | Commit `6199060` (merge `e3c684a`) | — |
| MSI candidato `0.0.2` (compilación local aislada) | `0.0.2` | `26.468.352` | `648230ADD71703F6CB104B7FBD3B2D6BE43BE46B8295D57ECA01B308B09E2478` | `{38AD1CF7-D2ED-429E-ADBD-8810DE20A234}` | `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}` |

> **Nota sobre alcance e identificadores MSI:** WiX genera un `ProductCode` (`Product Id="*"`) nuevo en cada empaquetado MSI manteniendo fijo el `UpgradeCode` (`{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}`). El MSI histórico `0.0.1` utilizado en el runner tiene `ProductCode = {E826D686-D784-4205-95C2-F19231569F02}` (documentado en `docs/VALIDACION-PRODUCTO-WINDOWS.md`), mientras que la instalación `0.0.1` actualmente registrada en el equipo personal del usuario procede de una compilación posterior (`15/10/2026`, `ProductCode = {33E76E35-45AF-48DA-8DC6-2A3CA2538BC7}`). Ambos paquetes `0.0.1` comparten exactamente el mismo `UpgradeCode` (`{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}`) y `ProductVersion` (`0.0.1`), que son las dos claves utilizadas por la tabla `Upgrade` de Windows Installer (`FindRelatedProducts` / `RemoveExistingProducts`).

---

## 3. Resultados detallados por categoría

### 3.1. Instalación limpia y desinstalación de `0.0.2` (Runs `40146425180` y `40551632816`)
- `msiexec /i` código `0` (`msi-clean-install.log`).
- `ProductState = 5`, entrada `Uninstall` con `DisplayName = "Orchestrator's Brain"`, `DisplayVersion = "0.0.2"`, `InstallLocation = "C:\Program Files\Orchestrator's Brain\"`.
- Binarios `obrain-app.exe` y `DirectML.dll` presentes; clave `HKCU:\Software\obrain\Orchestrator's Brain` creada.
- Arranque aislado durante 20 segundos (`processAliveAfter20Seconds = true`, `sessionId = 2`, `originalProfileBrainExists = false`).
- `msiexec /x` código `0` (`msi-clean-uninstall.log`), `ProductState = -1`, eliminación completa de binarios y registro.

### 3.2. Actualización mayor (`Major Upgrade`) `0.0.1 -> 0.0.2` en `windows-2022` (Run `40551632816`)
- **Creación de base auténtica `0.0.1`:** Generada antes de instalar `0.0.1` utilizando exclusivamente la CLI compilada desde `5421ceeeb977944edec8406119f504f8c8e0a17a` (`verifiedDistinctFrom002 = true`, `verifiedHistoricalSurfaceWithoutRespaldar = true`).
- **Instalación y arranque de `0.0.1`:** `msiexec /i` código `0` (`msi001-install.log`), `ProductState = 5`, `launch001.aliveAfter10s = true`.
- **Actualización a `0.0.2`:** `msiexec /i` código `0` (`msi-upgrade-to-002.log`):
  - `FindRelatedProducts` detectó `{E826D686-D784-4205-95C2-F19231569F02}` (`WIX_UPGRADE_DETECTED`).
  - `RemoveExistingProducts` desinstaló `0.0.1` (`Return value 1`, `productState001Post = -1`, `uninstall001Removed = true`).
  - `0.0.2` quedó registrado (`productState002Post = 5`, `uninstall002Registered = true`, `displayVersion002 = "0.0.2"`, `exeUpdated = true`).
- **Apertura del perfil `0.0.1` desde `0.0.2`:** `obrain-app.exe` `0.0.2` arrancó y permaneció vivo 20 segundos sobre la base `0.0.1` (`launch002.aliveAfter20s = true`, `isolatedProfileProtected = true`).
- **Desinstalación de `0.0.2`:** `msiexec /x` código `0` (`cleanup-{64052E5F-0B35-4D4A-949B-603A66556284}.log`, `Removal completed successfully`).

### 3.3. Conservación lógica de conocimiento e integridad SQLCipher (`5421cee` `0.0.1` -> `0.0.2`)
Ejecutada con el bloque corregido de `tools/validar-msi-runner.ps1` bajo `Set-StrictMode -Version Latest` en un perfil temporal aislado (`%TEMP%`), partiendo de una base creada por `obrain.exe` `0.0.1` (`5421cee`, `221.184 bytes`, `SHA-256: B9D8778066C9DD97E03D0A3F49581C4ED118F2FE1C962CFF23E99AAAA4D91C3B`) y consultada/verificada con `obrain.exe` `0.0.2`:
- `bibliotecaConservada = true` (`11` documentos de la biblioteca sintética).
- `paqueteAuroraConservado = true` y `anexoContenidoConservado = true` (relación `part_of`: `expediente-AURORA.txt` contiene `anexo-cita.txt`, `4 de 4 párrafos`).
- `carpetaSensibleConservada = true` (`05 identidad ficticia`, `1 documento`).
- `coleccionConservada = true` y `documentosEnColeccionConservados = true` (`Tramites Activos` con `expediente-AURORA.txt` y `↳ anexo-cita.txt`).
- `consultaVencimientosOperativa = true`.
- **Integridad SQLCipher y estructural (`obrain respaldar` + `obrain respaldo --verificar`):**
  - `encryptedHeaderVerifiedPre = true` y `encryptedHeaderVerifiedPost = true`.
  - `keyUnlockSuccessful = true` (`obrain clave` recupera la clave de recuperación y `verify_recovery_matches` valida la caja Argon2id/XChaCha20-Poly1305 contra la llave DPAPI).
  - `respaldarExitCode = 0` y `verificarExitCode = 0` (`Store::verify_snapshot` -> `opening::validate_integrity` ejecutó `PRAGMA quick_check`, `PRAGMA foreign_key_check`, `PRAGMA cipher_integrity_check` y `PRAGMA integrity_check` sin errores).
  - Instantánea verificada (`manifiesto.json`): `esquemaVersion = 2`, `entidades = 19`, `aserciones = 215` (`215` vigentes), `fragmentos = 11`, `vectores = 0`, `huellaContenido = 8076b2a035b7a19063866e4b9a8fd8cc2a34e2f5b274975d1d92e26f9eb2a345`.

### 3.4. Robustecimiento y garantías de CI (PR #16)
- **Aislamiento e identidad estricta de binarios CLI:** Se eliminó el riesgo de sobrescritura de `obrain.exe` en `CARGO_TARGET_DIR` garantizando que la CLI candidata se preserve en `obrain-cli-002\obrain.exe` y la CLI histórica en `obrain-cli-001\obrain.exe`. Un paso explícito de verificación en el workflow comprueba que ambos binarios existan, que sus hashes SHA-256 difieran y que su superficie de comandos (`obrain respaldar` ausente en 0.0.1 y presente en 0.0.2) sea exacta antes de iniciar la validación.
- **Calibración de timeout:** Se fijó `timeout-minutes: 70` en `.github/workflows/validar-msi-windows.yml` tras constatar que las compilaciones en frío completas de Tauri, Rust, WiX y SQLite en runners Windows 2022 tardan ~50 min, otorgando un margen prudencial de ~20 min que previene cancelaciones espurias en ejecuciones manuales autorizadas.

---

## 4. Salvaguarda de la instalación y perfil personal

- `%USERPROFILE%\.obrain\brain.obrain`: **Intacto y no accedido** (última modificación: `23/09/2026`).
- `C:\Program Files\Orchestrator's Brain\obrain-app.exe`: **Intacto y no modificado** (`0.0.1`, última modificación: `14/10/2026`).
- Registro de Windows (`Uninstall`): **Versión `0.0.1` (`{33E76E35-45AF-48DA-8DC6-2A3CA2538BC7}`) intacta y activa** en el anfitrión personal.
- **Cero ejecuciones de instalación/desinstalación `msiexec` ni de `obrain-app.exe` en el anfitrión.**
