# Prompt para la siguiente conversación — después de la carpeta que sigue a su `FileID`

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano; prefiero que te tardes y lo hagas bien.

## Antes de nada: sincroniza

- Trabajo desde dos sitios: sesiones en la nube (desde la PC de la escuela) y mi laptop con Windows. **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`**. Todavía no está fusionada a `main`.
- Si eres una sesión en la nube: haz `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq` (o `git pull` si ya estás en ella). Trabaja y sube **sólo** a esa rama. Si ya se fusionó a `main`, parte de `main`.
- Confirma con `git log --oneline -6` que el último commit es «Prompt para retomar después de la carpeta que sigue a su FileID» (o uno posterior), y `git status` limpio.
- Al terminar cada pieza: commit **y push**.

## Dónde estamos

- Lee primero `docs/ESTADO.md` y el ADR 0027 (la carpeta sigue a su `FileID`).
- Hecho en la sesión del 2026-09-23 en mi laptop:
  1. **La marca de carpeta ya no se pierde al renombrar** (ADR-0027, opción B): cada carpeta apunta su `folder_id` y, si aparece en otra ruta, la misma entidad se muda; la marca, el mapa y la historia van con ella.
  2. **Primera vez que la suite corre en Windows**: 400 tests en verde. Se arreglaron el vigía (podaba por encima de la biblioteca, y el `Modify(Any)` de la carpeta madre lo despertaba con el candado de Word) y dos tests que sólo pasaban en Linux.
  3. **DPAPI funciona de verdad** en mi laptop (probado con la orden `obrain`).
  4. Decisiones de §10: regiones del usuario **como colecciones**; `$Recycle.Bin` **no** se indexa; **un documento dentro de otro** con un predicado nuevo detectado por hashes de fragmento, sin IA. La Fase 4 sigue congelada.
- Si te paso errores o capturas de mi Windows, arréglalos antes que cualquier otra cosa.

## Lo que sigue, en este orden

0. **Fallo abierto: el cifrado de la base de agosto quedó a medias en mi Windows (2026-09-23).** Al abrir la app por primera vez, `%LOCALAPPDATA%\obrain` terminó con **dos archivos distintos llamados `brain.obrain`**: el de agosto en claro (FileID `…080000000ebbd5`, fechas de agosto), que era el que se veía por la ruta y el que abría la orden `obrain` («la base no se abre con esta llave»), y el cifrado de hoy (8,9 MB, FileID `…06000000134bf1`), que usaba la app y sólo apareció al mover el otro. El WAL era del cifrado. Con copias de las bases viejas, `obrain` cifra bien: lo distinto es la app (`tauri dev`). Sospecha: `reanudar` en `obrain-api/src/seguridad.rs` (el `rename` de Rust en Windows sobre un archivo que alguien tenía abierto). Todo está guardado en `%LOCALAPPDATA%\obrainespaldo-cifrado-a-medias-20260923\` (`cifrada-de-hoy\` con su llave y su WAL, `en-claro-de-agosto\`). Reprodúcelo antes de tocar el código.
1. **Probar la app en mi Windows** con mi bóveda de verdad (la lista está en `docs/ESTADO.md`). Lo que falle, primero.
2. **Un documento dentro de otro**: enséñame el diseño con su coste antes de programar, luego el ADR (0028), el código y los tests.
3. **Colecciones** (regiones del usuario): lo mismo.

## En mi laptop (Windows)

- Compilar necesita **Strawberry Perl** (ya instalado) y, por el apóstrofo de `Orchestrator's-Brain`, compilar **fuera** de la carpeta: `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target` (en Git Bash, además `PATH=/c/Strawberry/perl/bin:$PATH`, porque el Perl de Git no sirve). Sin eso, OpenSSL falla con `Unknown regexp modifier` en `configdata.pm`.
- Aquí sí se puede comprobar el código de Windows de verdad: `cargo test --workspace` corre en NTFS.

## Entorno en la nube

- **ONNX Runtime**: la descarga de `cdn.pyke.io` está bloqueada; no la esquives. `pip download onnxruntime --no-deps`, descomprime el wheel, copia `libonnxruntime.so.1.x` a un `lib/` con los enlaces `libonnxruntime.so.1` y `libonnxruntime.so`, y compila con `ORT_LIB_LOCATION=<dir>/lib ORT_PREFER_DYNAMIC_LINK=1 LD_LIBRARY_PATH=<dir>/lib`. Cada llamada a la terminal empieza de cero: pon las tres variables en un `env.sh` y haz `source` en cada una.
- **Tauri en Linux**: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`. En `ui/`, `npm install`.
- **Playwright** está en `/opt/node22/lib/node_modules` (`NODE_PATH`), con Chromium en `/opt/pw-browsers`; `chromium.launch()` sin más lo encuentra.
- **Código de Windows**: `rustup target add x86_64-pc-windows-msvc` y `cargo clippy -p obrain-keys -p obrain-ocr --target x86_64-pc-windows-msvc --all-targets -- -D warnings`. El núcleo entero no se puede comprobar así (SQLCipher necesita el compilador de C de Microsoft); para una dependencia o una llamada nueva de Windows, compruébala sola en un crate de prueba.
- **Banco de pruebas** (nada de esto va al repo; hay que rehacerlo en cada sesión, en el scratchpad):
  - Un crate aparte (`[workspace]` propio, dependencias por ruta a `core/obrain-api`, `obrain-core`, `obrain-scan`, más `tiny_http`, `serde`, `serde_json`), compilado con `CARGO_TARGET_DIR=core/target` para reutilizar lo compilado. Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__` (`invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script). Los eventos (`indexando`, `indexado`) salen de `GET /eventos`, que la página consulta cada 400 ms.
  - Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain` (incluidos `papelera` y `olvidar`). `actualizar` reindexa en un hilo con **otra conexión** (`seguridad::abrir`), como la app.
  - La base de pruebas se crea con `obrain indexar <bóveda> --db <scratchpad>/db/brain.obrain --sin-modelo` y después `obrain clave --guardada --db …`. La bóveda ficticia son `.txt` con datos inventados, incluidos los ejemplos de vencimientos («VALIDEZ 15 11 2026», «Fecha de caducidad: 12 MAR 2031», «vence el 30 de septiembre de 2026», «Vigencia: del 1 de septiembre de 2026 al 31 de agosto de 2027»). Para fotos: Pillow y fpdf2 en un venv, y Tesseract (`apt-get install tesseract-ocr tesseract-ocr-spa`) sólo en el puente.
- **Procesos**: `pgrep -f` y `pkill -f` se encuentran a sí mismos; usa `pgrep -x` / `pkill -x`.
- **Rutas de Windows**: no las escribas con barras invertidas dentro de strings de Python en un heredoc; usa la herramienta de edición. Tras `cargo fmt`, los reemplazos de texto por script pueden no casar: comprueba que se aplicaron.
- **Tests que también corren en Windows**: una ruta de prueba se arma con `join` trozo a trozo (no `lib.join("a/b.txt")`: el escáner guarda `\`), y no cuentes con empates de hora de modificación (NTFS guarda 100 ns).

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` y capturas antes de darla por buena.
- Las decisiones con coste, antes del código: enséñamelas y déjame elegir. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final, recuérdame lo que me toca hacer a mí en Windows (está en `docs/ESTADO.md`).
