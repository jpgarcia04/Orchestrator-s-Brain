# Prompt para la siguiente conversación — después de «un documento dentro de otro»

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano y en palabras sencillas; prefiero que te tardes y lo hagas bien.

## Antes de nada: sincroniza

- Trabajo desde dos sitios: sesiones en la nube (desde la PC de la escuela) y mi laptop con Windows. **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`**. Todavía no está fusionada a `main`.
- Si eres una sesión en la nube: `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq` (o `git pull` si ya estás en ella). Trabaja y sube **sólo** a esa rama. Si ya se fusionó a `main`, parte de `main`.
- Confirma con `git log --oneline -6` que el último commit es «Prompt para retomar después de un documento dentro de otro» (o uno posterior), y `git status` limpio.
- Al terminar cada pieza: commit **y push**.

## Dónde estamos

- Lee primero `docs/ESTADO.md` y el ADR 0030 (y el 0029 si no lo conoces).
- Hecho en la sesión del 2026-09-23 (noche) en mi laptop:
  1. **Un documento dentro de otro (ADR-0030).** Antes de programar se midió con mi base de agosto (una copia, sólo cuentas y nombres enmascarados): por **fragmentos** salía 1 de 14 paquetes; por **párrafos**, los 14 y ninguno falso. Lo elegí yo, junto con que **el paquete herede la sensibilidad** de lo que lleva y que en la búsqueda **lo de dentro vaya debajo** de su paquete. Regla: los párrafos de B (de 40 bytes o más) aparecen **de un tirón** en A (≥ 95 %, lo que sobre sólo en las orillas), A trae algo más (B ≤ 90 % de A) y B tiene al menos 3 párrafos. Eso separa un paquete de una versión (las versiones siguen congeladas). Predicado `part_of` (del de dentro al de fuera), fuente `linker:parrafos@v1`, evidencia `parrafos=…/… bytes=…..…` en el texto del paquete; los párrafos cortos de las orillas (un título) entran en el tramo marcado.
  2. En la app: apartado **«Contiene»** con su «¿por qué?» marcado en el texto, **«Está dentro de»** que abre el paquete por ese tramo (la vista previa acepta ahora un tramo), **«porque contiene …»** en la sensibilidad, y en el buscador lo de dentro metido debajo de su paquete. `obrain paquetes` en la terminal. El mapa dibuja la línea solo.
  3. 452 tests en verde en Windows; clippy y fmt limpios (núcleo y `ui/src-tauri`); `svelte-check` sin avisos; capturas en el banco de pruebas.
- **Todavía no he abierto la app con las versiones de hoy** (la carpeta fija de ADR-0029, los vencimientos sin palabra de ADR-0028 y esto). Al empezar la sesión del 2026-09-23 por la noche, `C:\Users\pacox\.obrain` sólo tenía `models`: la base no se había mudado.
- Si te paso errores o capturas de mi Windows, arréglalos antes que cualquier otra cosa.

## Lo que sigue, en este orden

1. **Comprobar en mi app lo de las últimas sesiones** (me toca abrirla; ver «Y para ver…» en `docs/ESTADO.md`). Si algo no sale, eso primero.
2. **Colecciones** (regiones del usuario): enséñame el diseño con su coste antes de programar, luego el ADR (**0031**; el 0030 ya se usó), el código y los tests. Lo decidido está en el maestro (§10): una entidad con faceta `region` creada por mí (I5), un documento en **varias**, el mapa se sigue dibujando por carpetas y la colección es una lente encima.

Pendientes pequeños, por si salen: el «¿por qué?» de la sensibilidad cita el nombre del archivo tal cual (con `%20`); lo que la interfaz guarda en el navegador (tema, apartados) sigue en AppData y puede variar según desde dónde se abra la app; en la copia privada de Claude quedan restos de agosto (`vida.obrain-wal`, `respaldo-cifrado-a-medias-20260923`) que sólo borro yo si quiero; un documento idéntico a uno que está en un paquete no dice «está dentro de» (lo dice su original); y un escaneo dentro de otro escaneo no se detecta (el OCR nunca lee dos veces igual).

## Si trabajas en mi laptop (Windows): cuidado con AppData

- **Tus terminales corren dentro de la app de Claude (MSIX).** Lo que ves en `%LOCALAPPDATA%` es una **mezcla** de la carpeta real y de `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Local`, y lo nuevo que escribas ahí va a la copia privada. `%TEMP%` y el perfil (`C:\Users\pacox\…`, incluido `.obrain`) **no** están virtualizados.
- **No corras `obrain` ni la app sin `--db`/`OBRAIN_DB` desde tus terminales**: la primera vez que obrain pregunta dónde está la base, muda lo que vea en `%LOCALAPPDATA%\obrain`, y desde Claude vería la mezcla. Para probar, `OBRAIN_DB=<copia>` o `--db`, y `USERPROFILE`/`LOCALAPPDATA` apuntando al scratchpad.
- Para ver la carpeta **real** (como la ve mi app), lanza el comando fuera del paquete con WMI: `Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'cmd.exe /c dir … > <archivo en el scratchpad>' }` y lee el archivo.
- **Mis documentos reales son míos**: para diagnosticar mi base, enmascara lo que imprimas (letras → `x`, cifras → `9`, salvo las etiquetas) y no copies datos personales a la conversación. Así se diagnosticó el pasaporte y así se midieron los paquetes. La base de agosto en claro es `vida.obrain` (con su `-wal`), en la mezcla de `%LOCALAPPDATA%\obrain`: cópiala al scratchpad antes de abrirla.
- Compilar necesita **Strawberry Perl** (ya instalado) y, por el apóstrofo de `Orchestrator's-Brain`, compilar **fuera** de la carpeta: `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target` (en Git Bash, además `PATH=/c/Strawberry/perl/bin:$PATH`). Sin eso, OpenSSL falla con `Unknown regexp modifier` en `configdata.pm`.
- `cargo test --workspace` corre en NTFS de verdad; úsalo.
- **No escribas rutas de Windows dentro de strings de Python en un heredoc** (`\0`, `\v`, `\U` rompen, y dejan caracteres de control invisibles en el archivo): usa la herramienta de edición. Pasó otra vez el 2026-09-23 con unos tests.
- `cargo fmt` a veces no puede escribir un archivo que OneDrive tiene abierto (`os error 1224`): repítelo.
- **El OCR de Windows (es-MX) casi nunca lee una MRZ** (probado con MRZ dibujadas en Consolas, Courier y Arial). No lo cuentes como fallo.

### Banco de pruebas en Windows (en el scratchpad; nada va al repo)

- Un crate aparte (`[workspace]` propio, dependencias por ruta a `core/obrain-api` y `core/obrain-core`, más `tiny_http`, `serde`, `serde_json`). **Copia `core/Cargo.lock` a su carpeta** antes de compilar, y `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target`, para reutilizar lo compilado. Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__` (`invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script); los eventos salen de `GET /eventos`, consultado cada 400 ms. Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain` (`previsualizar` con su `tramo` → `preview_at`).
- **El banco de la sesión anterior se queda en el scratchpad de esa sesión** (`%TEMP%\claude\<proyecto>\<sesión>\scratchpad\banco` y `boveda`): cópialo en vez de rehacerlo. Y si `banco.exe` no se deja recompilar («Acceso denegado»), es que el de otra sesión sigue corriendo: `Get-Process banco`.
- La bóveda ficticia: `.txt` con datos inventados (un «pasaporte» con la forma exacta del OCR del mío, una INE, una póliza, un contrato y su copia, una constancia de identidad, dos meses de movimientos y un «Expediente consulado» que los junta), imágenes dibujadas con `System.Drawing` para el OCR, y un `Zone.Identifier` puesto con `Set-Content -Stream Zone.Identifier` para «Descargado de». La base: `obrain indexar <bóveda> --db <scratchpad>\db\brain.obrain --sin-modelo`, `obrain clave --guardada --db …` y `obrain sensible "<bóveda>\01 Identidad" --db …`.
- **Capturas en el navegador integrado:** con tamaños emulados se cuelgan. Usa el tamaño del panel y, para que quepa el escritorio, `document.documentElement.style.zoom = 0.68` (se pierde al recargar). La acción `zoom` no existe ahí; para leer texto, `get_page_text` o `javascript_exec`. Con zoom CSS, `offsetTop` y `scrollTop` no se comparan bien: fíate de la captura.
- Un `Remove-Item` en PowerShell con rutas en variables lo bloquea el guardián aunque la ruta sea del scratchpad: crea y borra con Bash.

## Entorno en la nube

- **ONNX Runtime**: la descarga de `cdn.pyke.io` está bloqueada; no la esquives. `pip download onnxruntime --no-deps`, descomprime el wheel, copia `libonnxruntime.so.1.x` a un `lib/` con los enlaces `libonnxruntime.so.1` y `libonnxruntime.so`, y compila con `ORT_LIB_LOCATION=<dir>/lib ORT_PREFER_DYNAMIC_LINK=1 LD_LIBRARY_PATH=<dir>/lib`. Cada llamada a la terminal empieza de cero: pon las tres variables en un `env.sh` y haz `source` en cada una.
- **Tauri en Linux**: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`. En `ui/`, `npm install`.
- **Playwright** está en `/opt/node22/lib/node_modules` (`NODE_PATH`), con Chromium en `/opt/pw-browsers`.
- **Código de Windows**: `rustup target add x86_64-pc-windows-msvc` y `cargo clippy -p obrain-keys -p obrain-ocr --target x86_64-pc-windows-msvc --all-targets -- -D warnings`. El núcleo entero no se puede comprobar así (SQLCipher necesita el compilador de C de Microsoft).
- El banco de pruebas es el mismo que en Windows (arriba), con Tesseract (`apt-get install tesseract-ocr tesseract-ocr-spa`) en vez del OCR de Windows. `pgrep -f`/`pkill -f` se encuentran a sí mismos: usa `pgrep -x`/`pkill -x`.
- **Tests que también corren en Windows**: una ruta de prueba se arma con `join` trozo a trozo (el escáner guarda `\`), y no cuentes con empates de hora de modificación (NTFS guarda 100 ns).

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` y capturas antes de darla por buena.
- Las decisiones con coste, antes del código: enséñamelas **en palabras sencillas** y déjame elegir. **Mídelas con mis datos cuando se pueda** (enmascarado): así se vio que «por fragmentos» no servía. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final, recuérdame lo que me toca hacer a mí en Windows (está en `docs/ESTADO.md`).
