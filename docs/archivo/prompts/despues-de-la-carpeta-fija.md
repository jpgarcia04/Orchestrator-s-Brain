# Prompt para la siguiente conversación — después de la carpeta fija (`.obrain`)

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano y en palabras sencillas; prefiero que te tardes y lo hagas bien.

## Antes de nada: sincroniza

- Trabajo desde dos sitios: sesiones en la nube (desde la PC de la escuela) y mi laptop con Windows. **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`**. Todavía no está fusionada a `main`.
- Si eres una sesión en la nube: `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq` (o `git pull` si ya estás en ella). Trabaja y sube **sólo** a esa rama. Si ya se fusionó a `main`, parte de `main`.
- Confirma con `git log --oneline -6` que el último commit es «Prompt para retomar después de la carpeta fija» (o uno posterior), y `git status` limpio.
- Al terminar cada pieza: commit **y push**.

## Dónde estamos

- Lee primero `docs/ESTADO.md` y los ADR 0028 y 0029.
- Hecho en la sesión del 2026-09-23 (noche) en mi laptop:
  1. **Vencimientos sin palabra (ADR-0028).** Mi pasaporte (un PDF escaneado) no daba fecha: el OCR de Windows leyó las tres fechas pero se comió «caducidad» y «Date of expiry». Ahora hay tres lecturas más: **la zona MRZ** con su dígito de control (pesa más que cualquier palabra), **el plazo exacto** (dos fechas a 1–10 años justos, con año de cuatro cifras, sólo en documentos de identidad; así sale mi pasaporte: expedición + 10 años) y **el año solo** de la INE («VIGENCIA 2023 - 2033» → 31/12/2033). Detector `expiry@v2`. Comprobado contra mi base de agosto (561 documentos): ninguna falsa alarma.
  2. **Vista previa:** cada apartado (Documento, Lo que sé, Relacionado, Detalles técnicos) **se pliega desde su título y se recuerda**; «Descargado de» dice sólo el sitio (`chatgpt.com`), la URL entera está en Detalles técnicos; el título lee los `%20` de un archivo descargado.
  3. **El «cifrado a medias» no era un fallo del cifrado (ADR-0029).** La app de Claude y la de Codex son apps empaquetadas (MSIX): Windows les da su propia copia de `AppData`, y lo que se lanza desde ellas ve las dos carpetas mezcladas. Mi base de agosto y el modelo vivían en la copia de Claude; la app, abierta desde una PowerShell normal, creó otra base. **Ahora todo vive en `C:\Users\pacox\.obrain`** (fuera de AppData y de OneDrive) y obrain muda solo lo de `%LOCALAPPDATA%\obrain` la primera vez. El vigía ignora las carpetas con punto del perfil. El modelo ya se movió a mano a `C:\Users\pacox\.obrain\models`.
  4. 423 tests en verde en Windows.
- Si te paso errores o capturas de mi Windows, arréglalos antes que cualquier otra cosa.

## Lo que sigue, en este orden

1. **Comprobar en mi app lo de esta sesión** (me toca abrirla; ver abajo). Si algo no sale, eso primero.
2. **Un documento dentro de otro**: enséñame el diseño con su coste antes de programar, luego el ADR (**0030**; el 0028 y el 0029 ya se usaron), el código y los tests.
3. **Colecciones** (regiones del usuario): lo mismo (ADR 0031).

Pendientes pequeños, por si salen: el «¿por qué?» de la sensibilidad cita el nombre del archivo tal cual (con `%20`); lo que la interfaz guarda en el navegador (tema, apartados) sigue en AppData y puede variar según desde dónde se abra la app; en la copia privada de Claude quedan restos de agosto (`vida.obrain-wal`, `respaldo-cifrado-a-medias-20260923`) que sólo borro yo si quiero.

## Si trabajas en mi laptop (Windows): cuidado con AppData

- **Tus terminales corren dentro de la app de Claude (MSIX).** Lo que ves en `%LOCALAPPDATA%` es una **mezcla** de la carpeta real y de `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Local`, y lo nuevo que escribas ahí va a la copia privada. `%TEMP%` y el perfil (`C:\Users\pacox\…`, incluido `.obrain`) **no** están virtualizados.
- Para ver la carpeta **real** (como la ve mi app), lanza el comando fuera del paquete con WMI: `Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'cmd.exe /c dir … > <archivo en el scratchpad>' }` y lee el archivo.
- **Mis documentos reales son míos**: para diagnosticar mi base, enmascara lo que imprimas (letras → `x`, cifras → `9`, salvo las etiquetas) y no copies datos personales a la conversación. Así se diagnosticó el pasaporte.
- Compilar necesita **Strawberry Perl** (ya instalado) y, por el apóstrofo de `Orchestrator's-Brain`, compilar **fuera** de la carpeta: `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target` (en Git Bash, además `PATH=/c/Strawberry/perl/bin:$PATH`). Sin eso, OpenSSL falla con `Unknown regexp modifier` en `configdata.pm`.
- `cargo test --workspace` corre en NTFS de verdad; úsalo.
- No escribas rutas de Windows dentro de strings de Python en un heredoc (`\U`, `\o` rompen): usa la herramienta de edición.
- **El OCR de Windows (es-MX) casi nunca lee una MRZ** (probado con MRZ dibujadas en Consolas, Courier y Arial). No lo cuentes como fallo.

### Banco de pruebas en Windows (en el scratchpad; nada va al repo)

- Un crate aparte (`[workspace]` propio, dependencias por ruta a `core/obrain-api` y `core/obrain-core`, más `tiny_http`, `serde`, `serde_json`). **Copia `core/Cargo.lock` a su carpeta** antes de compilar, y `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target`, para reutilizar lo compilado. Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__` (`invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script); los eventos salen de `GET /eventos`, consultado cada 400 ms. Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain`.
- La bóveda ficticia: `.txt` con datos inventados (un «pasaporte» con la forma exacta del OCR del mío, una INE, una póliza, un contrato y su copia), imágenes dibujadas con `System.Drawing` para el OCR, y un `Zone.Identifier` puesto con `Set-Content -Stream Zone.Identifier` para «Descargado de». La base: `obrain indexar <bóveda> --db <scratchpad>\db\brain.obrain --sin-modelo` y `obrain clave --guardada --db …`.
- **Capturas en el navegador integrado:** con tamaños emulados se cuelgan. Usa el tamaño del panel y, para que quepa el escritorio, `document.documentElement.style.zoom = 0.68`. La acción `zoom` no existe ahí; para leer texto, `get_page_text` o `javascript_exec`.
- Para reproducir cosas de la app sin tocar mi base: `OBRAIN_DB=<copia>` (la app y `obrain` la respetan), o `USERPROFILE`/`LOCALAPPDATA` apuntando al scratchpad para probar la mudanza.
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
- Las decisiones con coste, antes del código: enséñamelas **en palabras sencillas** y déjame elegir. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final, recuérdame lo que me toca hacer a mí en Windows (está en `docs/ESTADO.md`).
