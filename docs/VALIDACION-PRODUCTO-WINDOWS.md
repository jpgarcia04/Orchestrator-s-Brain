# Primera experiencia integral de producto en Windows

Fecha: 13 de octubre de 2026. Base: `origin/main` en `5421cee` (PR #6 fusionada). Sólo se usaron documentos sintéticos. La bóveda de la persona usuaria y la instalación activa de Brain no se abrieron ni se modificaron.

## Resultado ejecutivo

**No es posible declarar la aplicación usable desde un MSI instalado con esta evidencia.** El equipo disponible es Windows `CoreSingleLanguage` (Home), no tiene `WindowsSandbox.exe` ni una VM de Windows accesible. Docker sólo expone la ruta de su motor Linux y el demonio no estaba en marcha. Una carpeta temporal y variables de entorno no aíslan las modificaciones que `msiexec` puede hacer a Windows Installer, Program Files, accesos directos y registro. Por tanto, no se ejecutó el MSI, no se inició una aplicación instalada, no hubo clics de interfaz y no se probó la desinstalación. No se sustituye esa prueba por resultados de endpoints Rust.

La biblioteca y las pruebas automáticas permiten repetir el ensayo en una VM desechable. No apareció un defecto funcional de Brain en los casos automáticos comprobados. La ausencia de una VM sigue siendo el bloqueo principal de **validación**, no un fallo observado del producto.

El alcance de esta PR termina en la biblioteca sintética, sus regresiones y este informe. La instalación real del MSI y los ocho recorridos visuales forman una etapa posterior independiente.

## Preparación, aislamiento y paquete

- `ui/src-tauri/tauri.conf.json` declara Tauri 2, producto `Orchestrator's Brain`, versión `0.0.1`, identificador `dev.obrain.app` y bundle activo. La orden de esta validación solicita **sólo MSI** (`--bundles msi`); no intenta corregir NSIS. La PR #5 documentó un fallo de macro de NSIS y un MSI cuya carga extraída arrancó, sin instalación.
- `npm run tauri -- build --ci --no-sign --bundles msi` produjo `Orchestrator's Brain_0.0.1_x64_en-US.msi` (26.234.880 bytes, SHA-256 `9AEE1B8125CC19DDBBF124350CD86EB510E77A9313C89094E0797A6AF7E35591`). `Get-AuthenticodeSignature` devolvió `NotSigned`. Se abrió la base MSI en modo lectura mediante Windows Installer COM y se extrajo la carga con `dark.exe`; las tablas `File` contienen `obrain-app.exe` (46.030.848 bytes) y `DirectML.dll` (18.527.776 bytes). La carga del ejecutable difiere del binario release sólo en los tres bytes del marcador de bundle `UNK` → `MSI`.
- Las tablas del MSI declaran `ALLUSERS=1`, `INSTALLDIR` bajo `ProgramFiles64Folder`, accesos directos de Escritorio y menú Inicio, y cuatro entradas bajo `Software\obrain\Orchestrator's Brain` en el registro del usuario. `RemoveExistingProducts` está en la secuencia de instalación. No hay tabla `ServiceInstall`. Una acción `DownloadAndInvokeBootstrapper`, condicionada a `NOT(REMOVE OR INSTALLED_WEBVIEW2_VERSION)`, descarga `MicrosoftEdgeWebview2Setup.exe` desde Microsoft a `%TEMP%` y lo ejecuta con `/silent /install`. Es un efecto adicional potencial en un invitado limpio; no se ejecutó ni se comprobó su comportamiento real. El MSI ofrece también una opción de lanzar la aplicación al finalizar.
- La base por defecto vive en `%USERPROFILE%\.obrain\brain.obrain`, con la llave a su lado y modelos bajo `.obrain\models`; la ruta antigua `%LOCALAPPDATA%\obrain` puede migrarse en el primer arranque. `OBRAIN_DB` prevalece para pruebas. El WebView2 mantiene datos de usuario propios. Estas rutas deben pertenecer al **perfil del invitado** y su disco debe ser desechable antes de instalar. El MSI además puede tocar ubicaciones del sistema que `OBRAIN_DB` no controla.
- Se comprobó `EditionID=CoreSingleLanguage`; no se encontró el ejecutable de Windows Sandbox, `Get-VM`, VirtualBox, VMware ni QEMU. `wsl --list` mostró `docker-desktop`, que no es un invitado Windows para esta prueba. `docker info` no pudo conectar al motor Linux. No se ha demostrado aislamiento efectivo de instalación, registro ni perfiles: por eso no se ejecutó `msiexec`.
- La fixture se copia sólo bajo `%TEMP%` mediante `tools/biblioteca-sintetica.ps1`. El script rechaza una raíz fuera de esa zona y una segunda fase sobre archivos que no coinciden con los originales. Su prueba de preparación creó 11 documentos y aplicó el cambio; una ruta bajo `%USERPROFILE%\.obrain` fue rechazada como se esperaba.

## Biblioteca y oráculo

`tests/fixtures/producto-windows` contiene 11 archivos de texto, Markdown, CSV, HTML y PDF: estudios, trámites, contrato, finanzas e identidad ficticia. `tools/generar-pdf-fixture.py` regenera el PDF de una página; se renderizó y se revisó visualmente, y `pdfplumber` extrajo el texto. La tarjeta `4111 1111 1111 1111` es un número público de pruebas y no una credencial. No hay identificadores personales auténticos.

| Escenario | Esperado | Resultado automático observado |
| --- | --- | --- |
| Recuerdo parcial | `nodo azafrán` encuentra los apuntes | Sí, por `bm25`; `semantic=false` |
| PDF | Vista previa con texto de laboratorio | Sí, se extrajo `laboratorio de redes` |
| Procedencia | La ficha del anexo justifica `part_of` | Sí, aserción observada con evidencia |
| Copias | Dos recibos iguales tienen un `duplicate_of` | Sí, una relación activa |
| Contención | El anexo pertenece al expediente AURORA | Sí, `part_of` apunta al expediente |
| Colección y filtro | Tres documentos; búsqueda `matrícula` dentro de ella | Sí, tres filas y PDF recuperado, sin resultados ajenos |
| Sensibilidad | Pasaporte por nombre y tarjeta de prueba por contenido | Ambos con nivel `sensitive` |
| PIN | Con PIN bloqueado no sale texto sensible; al desbloquear sí | Sí en API de núcleo |
| Cambio posterior | Nuevos apuntes; contrato con misma identidad; copia deja de ser duplicada | Sí tras reindexar en el test de integración |

El oráculo está en `core/obrain-api/tests/producto_windows.rs` y usa el pipeline de indexación real sobre una copia temporal y un `Store` en memoria. No prueba persistencia entre procesos, temporizadores visuales del PIN ni interacción con ventanas.

Para repetirlo desde la raíz del repositorio en Windows, con Rust disponible y `CARGO_TARGET_DIR` fuera de la ruta del repositorio que contiene un apóstrofo:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'obrain-cargo-target'
cargo test --manifest-path core/Cargo.toml --locked -p obrain-api --test producto_windows
```

## Recorridos de interfaz e instalación

| Recorrido solicitado | Estado | Evidencia que falta |
| --- | --- | --- |
| Instalar MSI, iniciar, desinstalar | No ejecutado | Registro de instalación y desinstalación en invitado desechable; rutas antes/después |
| Primer arranque y registro de biblioteca | No ejecutado | Interacción y captura de ventana instalada |
| Buscar parcialmente y abrir vista previa | No ejecutado en interfaz | Clics, texto visible y procedencia mostrada |
| Relaciones, duplicados e incertidumbre | No ejecutado en interfaz | Pantalla que distinga vínculos probados de coincidencias temáticas |
| Crear colección, añadir y filtrar | No ejecutado en interfaz | Flujo completo de la colección instalada |
| Modificar, renombrar, mover y actualizar | No ejecutado en interfaz | IDs y resultados antes/después en ventana instalada |
| Sensibilidad y PIN | No ejecutado en interfaz | Bloqueo, desbloqueo, caducidad y reintento visibles |
| Cerrar y reabrir | No ejecutado en interfaz | Biblioteca, colección y decisiones persistidas |
| Búsqueda semántica | No ejecutada | Modelo presente dentro del invitado y rama `knn` confirmada |

## Pruebas ejecutadas

| Tipo | Comando o comprobación | Resultado |
| --- | --- | --- |
| Automática | `cargo test --manifest-path core/Cargo.toml --locked -p obrain-api --test producto_windows` desde la raíz | 2/2 casos pasaron en Windows |
| Automática | `npm test` en `ui` | 12/12 pruebas pasaron |
| Automática | `npm run build` en `ui` | 0 errores y 0 avisos de Svelte; Vite construyó assets |
| Automática, empaquetado | `npm run tauri -- build --ci --no-sign --bundles msi` en `ui` | MSI x64 generado; sin instalación |
| Manual, lectura del MSI | Tablas Windows Installer y extracción `dark.exe` | Instalación por máquina, accesos directos, registro y bootstrapper WebView2 condicional identificados |
| Automática | `cargo fmt --all -- --check` | Pasó tras formatear la nueva prueba |
| Manual, artefacto | Render de PDF y lectura de una página | Legible, sin cortes; texto extraíble |
| Manual, preparación | `biblioteca-sintetica.ps1` en fases Inicial y Cambio con Windows PowerShell 5.1 | 11 documentos; cambios y acentos conservados; ruta real de Brain rechazada |
| Manual, producto instalado | MSI, ventanas, clics, desinstalación | No ejecutado por falta de invitado aislado |

## Prioridad y siguiente ensayo

1. **P0 de validación:** disponer de Windows Sandbox, VM desechable o equipo Windows remoto aislado. Antes de pasar el MSI, verificar que el invitado no comparte `%USERPROFILE%`, `%LOCALAPPDATA%`, `%APPDATA%`, `%TEMP%`, `.obrain` ni la biblioteca personal con el host. Compartir sólo MSI y fixture, idealmente en modo lectura. Registrar rutas, registro y accesos directos antes y después de instalar y desinstalar; comprobar además si el invitado necesita descargar WebView2.
2. **P1 de producto, pendiente de comprobar:** realizar los ocho recorridos en la ventana instalada con esta fixture. Registrar capturas, consulta, resultado esperado y obtenido, y mensajes confusos. Priorizar cualquier error documental, fuga de contenido sensible o pérdida de decisiones.
3. **P2 condicionado:** probar la rama semántica únicamente si el modelo está en el invitado. La prueba actual acredita sólo búsqueda léxica.

La próxima decisión sobre utilidad cotidiana debe apoyarse en esos recorridos visuales. La hipótesis más valiosa para evaluar es si una búsqueda por recuerdo parcial explica suficientemente **por qué** devolvió cada documento y permite continuar a su contexto y colección sin perderse.
