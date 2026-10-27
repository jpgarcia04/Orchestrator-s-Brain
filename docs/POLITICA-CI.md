# Política Permanente de Eficiencia en CI y Validación — Orchestrator's Brain

Esta política establece una separación estricta en **tres niveles** para proteger simultáneamente la calidad del producto, la bóveda personal del anfitrión y el consumo de minutos facturables en GitHub Actions.

---

## 1. Motivación y coste real en GitHub Actions

En repositorios privados de GitHub, los runners hospedados `windows-2022` aplican un **multiplicador de facturación de 2x** frente a `ubuntu-latest` (`1x`).
Una ejecución completa sin caché que compile desde cero SQLCipher (`libsqlite3-sys` con OpenSSL embebido), ONNX Runtime (`ort`), Tauri, WebView2 y el paquete MSI consume entre **31 y 36 minutos de reloj** (~62 a 72 minutos facturables equivalentes).

Por tanto, **está prohibido utilizar runners Windows o compilaciones completas de instaladores MSI como disparador automático de pull requests ordinarias**.

---

## 2. Arquitectura de verificación en tres niveles

### Nivel A — Desarrollo cotidiano (Entorno local del agente / desarrollador)
- **Dónde se ejecuta:** Localmente antes de abrir o actualizar una PR.
- **Comando unificado:**
  ```powershell
  ./tools/verificar-local.ps1 -Completo
  ```
- **Qué comprueba (sin tocar `%USERPROFILE%\.obrain` ni ejecutar `msiexec`):**
  1. Coherencia de versiones entre los 10 crates de `core/`, `ui/src-tauri/Cargo.toml`, `ui/src-tauri/tauri.conf.json`, `ui/package.json` y `ui/package-lock.json`.
  2. Sintaxis AST de todos los scripts PowerShell en `tools/*.ps1`.
  3. Cumplimiento de las invariantes de eficiencia en `.github/workflows/*.yml`.
  4. Formato de código Rust (`cargo fmt --all -- --check` en `core/` y `ui/src-tauri/`).
  5. Tipado Svelte/TypeScript (`npm run check`), pruebas unitarias de UI (`npm test`) y empaquetado Vite (`npm run build`).
  6. Pruebas unitarias e integración dirigida en Rust (`cargo test` sobre los crates modificados, utilizando un `CARGO_TARGET_DIR` aislado).

### Nivel B — Verificación automática de Pull Requests (`.github/workflows/verificar-pr.yml`)
- **Dónde se ejecuta:** `ubuntu-latest` (multiplicador `1x`).
- **Cuándo se dispara:**
  - Automáticamente en `pull_request` hacia `main` cuando no es `draft` y cuando los cambios no se limitan a `docs/**` o `**/*.md`.
  - Manualmente mediante `workflow_dispatch` (con opción `ejecutar_pruebas_rust` para correr `cargo test` completo en Linux con `Swatinem/rust-cache@v2`).
- **Duración objetivo:** `< 1 minuto` (~30–45 segundos).
- **Qué comprueba automáticamente:**
  - `./tools/verificar-local.ps1 -ModoCI` (coherencia de versiones, sintaxis AST PowerShell, invariantes de workflows CI, `cargo fmt --check`, `svelte-check`, `npm test` y `vite build`).
- **Control de consumo:**
  - `concurrency` con `cancel-in-progress: true` para cancelar inmediatamente ejecuciones obsoletas si se envían nuevos commits.
  - `timeout-minutes: 10`.
  - Caché de dependencias npm (`actions/setup-node@v4`).

### Nivel C — Distribución y actualizaciones MSI (`.github/workflows/validar-msi-windows.yml`)
- **Dónde se ejecuta:** `windows-2022` (entorno Windows desechable).
- **Cuándo se dispara:** **Exclusivamente de forma manual (`workflow_dispatch`)** cuando se prepara o valida formalmente un candidato de versión (ej. `0.0.2`).
- **Controles de eficiencia y reutilización:**
  1. **Preflight rápido antes de compilar (< 20 s):** Verifica la coherencia de versiones, la existencia del commit histórico `0.0.1` (`5421cee`) y descarga/verifica el `SHA-256` del MSI `0.0.1` **antes** de iniciar cualquier compilación de Rust o Tauri. Si falta un insumo externo, el trabajo falla en segundos en lugar de desperdiciar 35 minutos de compilación.
  2. **Reutilización de binarios del candidato por hash de código fuente:** Cachea el MSI y `obrain.exe` generados bajo la clave `hashFiles('core/**', 'ui/**')`. Si se reejecuta el workflow tras ajustar únicamente scripts de prueba (`tools/*.ps1`) o configuración del workflow, se reutilizan los binarios exactos del commit sin recompilar Tauri ni Rust.
  3. **Caché inmutable de la CLI histórica `0.0.1`:** El binario `obrain.exe` compilado desde el commit fijo `5421ceeeb977944edec8406119f504f8c8e0a17a` se conserva en caché por su SHA inmutable, evitando recompilar `0.0.1` (~10 min) en cada ensayo.
  4. **Caché incremental de Rust (`Swatinem/rust-cache@v2`) y npm:** Reduce el tiempo de compilación cuando cambia el código de `core/` o `ui/`.
  5. **Concurrencia y límite:** `cancel-in-progress: true` y `timeout-minutes: 45`.

---

## 3. Requisitos obligatorios para fusionar vs distribuir

| Tipo de cambio | Nivel A (Local) | Nivel B (`verificar-pr.yml` en Linux) | Nivel C (`validar-msi-windows.yml` en Windows) |
|---|---|---|---|
| Cambio ordinario en `core/` o `ui/` | **Obligatorio** (`verificar-local.ps1` + tests del crate) | **Obligatorio** (automático, < 1 min) | **No ejecutar** |
| Cambio exclusivo en `docs/` o `*.md` | **Obligatorio** (`verificar-local.ps1 -SoloPoliticaCI`) | Omitido automáticamente (`paths-ignore`) | **No ejecutar** |
| Cambio en scripts `tools/*.ps1` o CI | **Obligatorio** (`verificar-local.ps1`) | **Obligatorio** | Solo si modifica la lógica de instalación MSI (reutiliza caché de binarios) |
| Candidato de nueva versión / release MSI | **Obligatorio** (`verificar-local.ps1 -Completo` + `verificar-candidato-msi.ps1`) | **Obligatorio** | **Obligatorio** (1 ejecución manual `workflow_dispatch` al cerrar el candidato) |
