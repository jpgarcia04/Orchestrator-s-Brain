# Candidato Windows 0.0.2 — Orchestrator's Brain

Fecha: 25 de octubre de 2026.  
Base: `origin/main` en `ccd5251af99594c61b89c0f42dfea071ade22399` (PR #15 fusionada).  
Rama: `release/candidato-windows-0.0.2`.

## 1. Resumen ejecutivo

Se ha preparado coherentemente la versión **0.0.2** de Orchestrator's Brain en todos los componentes del proyecto (10 crates de Rust del workspace `core/`, la aplicación Tauri `ui/src-tauri` y los paquetes npm del frontend en `ui/`).

Se compiló exitosamente el paquete candidato MSI en un entorno de compilación aislado sin ejecutar operaciones de instalación ni registro en el sistema anfitrión. Se recuperó e inspeccionó el paquete MSI auténtico de la versión 0.0.1 previa, confirmando la compatibilidad estructural de actualización mayor (*Major Upgrade*) de Windows Installer.

Debido a que GitHub Actions se encuentra bloqueado por límites de facturación de la cuenta (*«The job was not started because recent account payments have failed or your spending limit needs to be increased»*), las pruebas que dependen de la invocación de `msiexec` se han estructurado en el runner y adaptado para actualización real, pero permanecen formalmente **pendientes de ejecución** en un entorno Windows desechable para proteger estrictamente la instalación personal del anfitrión.

---

## 2. Metadatos del candidato 0.0.2 y paquete 0.0.1

### Candidato 0.0.2 generado
- **Archivo:** `Orchestrator's Brain_0.0.2_x64_en-US.msi`
- **Ubicación:** `C:\Users\pacox\obrain-target\release\bundle\msi\Orchestrator's Brain_0.0.2_x64_en-US.msi`
- **Tamaño:** `26.468.352 bytes`
- **Hash SHA-256:** `648230ADD71703F6CB104B7FBD3B2D6BE43BE46B8295D57ECA01B308B09E2478`
- **Firma Authenticode:** `NotSigned` (generado deliberadamente con `--no-sign`)
- **ProductVersion:** `0.0.2`
- **ProductCode:** `{38AD1CF7-D2ED-429E-ADBD-8810DE20A234}`
- **UpgradeCode:** `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}`
- **ALLUSERS:** `1` (instalación por máquina en Program Files)
- **Binarios empaquetados:**
  - `obrain-app.exe`: `46.728.704 bytes`
  - `DirectML.dll`: `18.527.776 bytes`
- **Tabla ServiceInstall:** Ausente (sin servicios en segundo plano)

### Paquete 0.0.1 auténtico de referencia
- **Archivo:** `Orchestrator's Brain_0.0.1_x64_en-US.msi`
- **Ubicación de respaldo:** `C:\Users\pacox\obrain-msi-0.0.1\Orchestrator's Brain_0.0.1_x64_en-US.msi`
- **Tamaño:** `26.234.880 bytes`
- **Hash SHA-256:** `9AEE1B8125CC19DDBBF124350CD86EB510E77A9313C89094E0797A6AF7E35591` (coincide exactamente con el hash histórico documentado en `docs/VALIDACION-PRODUCTO-WINDOWS.md`)
- **ProductVersion:** `0.0.1`
- **ProductCode:** `{E826D686-D784-4205-95C2-F19231569F02}`
- **UpgradeCode:** `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}`

### Compatibilidad estructural de actualización
| Criterio Windows Installer | Valor 0.0.1 | Valor 0.0.2 | Resultado |
|---|---|---|---|
| `UpgradeCode` idéntico | `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}` | `{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}` | Cumple (misma familia) |
| `ProductCode` distinto | `{E826D686-D784-4205-95C2-F19231569F02}` | `{38AD1CF7-D2ED-429E-ADBD-8810DE20A234}` | Cumple (permite sustitución) |
| `ProductVersion` superior | `0.0.1` | `0.0.2` | Cumple (`0.0.2 > 0.0.1`) |
| Regla `WIX_UPGRADE_DETECTED` | Presente en `Upgrade` | Presente en `Upgrade` | Cumple (dispara desinstalación de versión previa) |

---

## 3. Entorno de compilación y herramientas

- **Sistema operativo:** Windows 10 Pro / Single Language `10.0.26300 x86_64`
- **Node.js:** `v24.16.0`
- **npm:** `11.13.0`
- **Rustc:** `1.97.1 (8bab26f4f 2026-07-14)`
- **Cargo:** `1.97.1 (c980f4866 2026-06-30)`
- **Tauri CLI:** `2.11.4`
- **WiX Toolset:** WiX 3.14.1 (descargado y cacheado localmente por Tauri durante el empaquetado)
- **Directorio de compilación (`CARGO_TARGET_DIR`):** `C:\Users\pacox\obrain-target`

---

## 4. Pruebas ejecutadas vs pruebas pendientes

### Pruebas ejecutadas
1. **Actualización de versión coherente:**
   - 10 crates en `core/` actualizados a `version = "0.0.2"`.
   - `core/Cargo.lock` actualizado con `0.0.2`.
   - `ui/src-tauri/Cargo.toml` y `ui/src-tauri/Cargo.lock` actualizados a `0.0.2`.
   - `ui/src-tauri/tauri.conf.json` actualizado a `"version": "0.0.2"`.
   - `ui/package.json` y `ui/package-lock.json` actualizados a `"version": "0.0.2"`.
2. **Prueba unitaria e integración de coherencia:**
   - `cargo test --manifest-path core/Cargo.toml -p obrain-api --test version_coherente` ejecutado exitosamente (`1 passed; 0 failed`).
3. **Pruebas de frontend:**
   - `npm test` en `ui`: 23 pruebas pasaron (0 fallos).
   - `npm run build` en `ui`: `svelte-check` con 0 errores y 0 avisos; compilación Vite completada.
4. **Compilación de candidato MSI:**
   - `npm run tauri -- build --ci --no-sign --bundles msi` generó `Orchestrator's Brain_0.0.2_x64_en-US.msi`.
5. **Inspección COM estática de paquete:**
   - `tools/verificar-candidato-msi.ps1` ejecutado exitosamente sin privilegios administrativos ni escrituras en el sistema, validando tablas, propiedades y correlación con el MSI 0.0.1.

### Pruebas pendientes (exclusivas de entorno desechable)
1. **Instalación limpia en runner Windows temporal (`msiexec /i`):**
   - Pendiente de que GitHub Actions disponga de saldo de ejecución o de un Windows Sandbox / VM independiente.
2. **Actualización real desde 0.0.1 (`msiexec /i 0.0.1` -> `msiexec /i 0.0.2`):**
   - El script `tools/validar-msi-runner.ps1` está preparado y adaptado para ejecutar la secuencia completa y verificar la recuperación de conocimiento (`part_of`, `duplicate_of`, marcas sensibles y bibliotecas).
   - Pendiente de ejecución en runner Windows desechable.
3. **Desinstalación limpia (`msiexec /x`):**
   - Pendiente de ejecución en runner Windows desechable.

---

## 5. Salvaguarda de la instalación y perfil personal

Se verificó el estado de las rutas y registros del sistema anfitrión antes y después de todas las tareas:
- `%USERPROFILE%\.obrain`: **Intacto y no modificado** (fecha de última modificación: 24/09/2026).
- `%USERPROFILE%\.obrain\brain.obrain`: **Intacto y no accedido**.
- `C:\Program Files\Orchestrator's Brain\obrain-app.exe`: **Intacto y no modificado** (fecha de última modificación: 14/10/2026, 46.060.544 bytes).
- Registro de Windows (`Uninstall`): **Versión 0.0.1 intacta y activa** en el anfitrión personal.
- **Cero ejecuciones de `msiexec` en el anfitrión.**
