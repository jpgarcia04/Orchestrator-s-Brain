# MSI de Brain en un runner Windows desechable

Base: `main` en `0c0365d14009d806226d40a260be44c8bfb7f147` (PR #7 fusionada). Esta PR añade un ensayo de instalación y desinstalación para el MSI generado por el mismo commit que ejecuta el workflow. No usa la bóveda personal, no instala en el equipo principal y no publica el paquete.

## Alcance y límites del entorno

El job usa `windows-2022`, un runner **hospedado por GitHub**. GitHub documenta que los runners Windows se ejecutan como administradores con UAC desactivado y que cada job normal recibe una VM nueva. Eso permite probar cambios de Windows Installer en un sistema que se desecha al terminar, pero no demuestra que haya un escritorio interactivo utilizable. La imagen y sus herramientas cambian con el tiempo. Se registran su versión, sistema operativo, Node, Rust y commit en la ejecución. Fuentes: [runners hospedados](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), [imágenes Windows](https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md).

`msiexec` admite `/i` para instalar, `/x` para desinstalar, `/qn` para no mostrar UI, `/norestart` para evitar reinicio y `/L*V` para registro detallado. Sus códigos de salida son parte del resultado observado; `3010` señala que hace falta reiniciar y no se acepta como prueba de instalación completa en este job. Fuente: [Microsoft, msiexec](https://learn.microsoft.com/windows-server/administration/windows-commands/msiexec).

El MSI de Tauri puede descargar y ejecutar el bootstrapper de WebView2 si falta el runtime. El script lee la tabla `CustomAction`, la condición en `InstallExecuteSequence` y la versión registrada de WebView2 **antes** de instalar. Si el paquete declara esa acción y no consta el runtime, se detiene: no descarga el bootstrapper. Una clave de registro no acredita que WebView2 vaya a renderizar en la sesión del runner. Fuentes: [Tauri, instalador Windows](https://v2.tauri.app/distribute/windows-installer/), [Microsoft, distribución de WebView2](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution).

## Ejecución reproducible

El workflow `.github/workflows/validar-msi-windows.yml` se activa para esta PR y admite ejecución manual una vez que exista en la rama predeterminada. Tiene `contents: read`, checkout sin persistir credenciales, sin secretos, despliegues ni pasos de publicación. Usa `npm ci`, `npm run tauri -- build --ci --no-sign --bundles msi` y `cargo build --manifest-path core/Cargo.toml --locked --release -p obrain-api --bin obrain`. El MSI queda asociado al SHA del job; se guardan tamaño, SHA-256, firma, versión y `ProductCode`.

`tools/validar-msi-runner.ps1` sólo continúa si detecta un runner hospedado, token administrador, `msiexec` y ausencia de carpetas Brain en el perfil original del runner. Abre el MSI en modo lectura mediante Windows Installer COM e inspecciona propiedades, archivos, directorios, registro, accesos directos, acciones personalizadas, secuencia y ausencia de `ServiceInstall`. Verifica producto, `ALLUSERS=1`, carga `obrain-app.exe` y `DirectML.dll`, y destino `ProgramFiles64Folder` antes de instalar.

La biblioteca de PR #7 se copia a una raíz nueva bajo `%TEMP%` con `tools/biblioteca-sintetica.ps1` y se comprueban sus 11 archivos. La CLI del mismo commit registra sólo esa biblioteca en una base cifrada dentro de un perfil sintético. Su salida se suprime para no exponer la clave de recuperación. Para el arranque de la aplicación instalada, el proceso hijo recibe `OBRAIN_DB`, `USERPROFILE`, `HOME`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP` y `WEBVIEW2_USER_DATA_FOLDER` dirigidos a esa raíz. Se comprueba que no aparecen `.obrain` en las dos ubicaciones originales del runner. **Esto comprueba rutas configuradas y escrituras visibles; no hay una traza completa de lecturas de archivos.**

El script ejecuta `msiexec /i`, verifica la entrada de desinstalación, `obrain-app.exe`, `DirectML.dll` y la clave de registro de la aplicación. Intenta iniciar el ejecutable durante 20 segundos, registra si el proceso sigue vivo, su sesión, la base y los datos del perfil WebView2, y lo cierra. En `finally`, intenta `msiexec /x` incluso si una comprobación posterior a la instalación falla; verifica código de salida y ausencia de registro de desinstalación y ejecutable. Guarda `resultado.json` y los logs detallados de MSI como artefacto del job durante siete días. No sube la base, su llave, la fixture copiada ni el perfil WebView2.

## Resultado observado

Pendiente de la primera ejecución del workflow de esta PR. El informe se actualizará con el SHA, enlace del job, códigos de salida, comprobaciones y bloqueos reales antes de cerrar la PR.

## Lo que el ensayo no acredita

Un proceso arrancado no equivale a una ventana dibujada. No se ejecutan clics, capturas ni los ocho recorridos visuales de `docs/VALIDACION-PRODUCTO-WINDOWS.md`. Tampoco se prueba un instalador firmado, actualización de versión, otras ediciones de Windows, perfiles reales, búsqueda semántica con modelo, lectura exhaustiva de rutas por el proceso, ni ausencia de efectos de Windows Installer fuera de las ubicaciones inspeccionadas. El runner se desecha al terminar.
