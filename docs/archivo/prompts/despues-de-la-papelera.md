# Prompt para la siguiente conversación — después de la papelera

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano; prefiero que te tardes y lo hagas bien.

## Antes de nada: sincroniza

- Trabajo desde dos sitios: sesiones en la nube (desde la PC de la escuela) y mi laptop. **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`**. Todavía no está fusionada a `main`.
- Si eres una sesión en la nube: el repo se clona de cero y quizá te deje en otra rama. Haz `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq` (o `git pull` si ya estás en ella). Trabaja y sube **sólo** a esa rama. Si ya se fusionó a `main`, parte de `main`.
- Confirma con `git log --oneline -6` que el último commit es «Fase 3.8: el «¿por qué?» de la sensibilidad, en palabras y con su tramo» (o uno posterior), y `git status` limpio.
- Al terminar cada pieza: commit **y push**. Lo que no se sube se pierde cuando la sesión en la nube se cierra.

## Dónde estamos

- Lee primero `docs/ESTADO.md` y los ADR 0025 (papelera), 0026 (fecha a mano) y 0020 (sensibilidad por carpeta).
- Hecho en la sesión del 2026-09-23:
  1. **Papelera (ADR-0025).** Lo que borro sale de todas partes y pasa 30 días a la vista «Papelera»; después se olvida su texto (o antes, con «Olvidar ya» / «Vaciar papelera»). El log se queda. Si vuelve (mismo FileID o mismo contenido), vuelve con todo. Mover y renombrar ya no dejan documentos ni carpetas repetidos. `obrain papelera [--vaciar]`.
  2. **Fecha de caducidad a mano (ADR-0026):** «Caduca el — añadir», en gris, donde el detector no vio ninguna.
  3. **«¿Por qué?» de la sensibilidad** en palabras de persona, con el tramo (el NIE) marcado en el texto. El tramo se recalcula al enseñarlo; el log sigue guardando sólo la dirección.
- 383 tests en verde, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check` y `npm run build` limpios. La Fase 4 sigue congelada.
- Si te paso errores o capturas de mi Windows, arréglalos antes que cualquier otra cosa.

## Lo que sigue, en este orden

4. **La marca de carpeta se pierde al renombrar la carpeta** (ADR-0020, «En contra»). Ya se investigó; **enséñame estas opciones con su coste y déjame elegir** antes de programar:
   - **A · Guardar el `FileID` de cada carpeta de la bóveda y mover la marca con él** (recomendada). En NTFS un directorio tiene `FileID` como un archivo (se abre con `FILE_FLAG_BACKUP_SEMANTICS` y `GetFileInformationByHandle`, lo mismo que ya hace `obrain-scan/src/identity.rs`), y se conserva al renombrar y al mover dentro del mismo disco. El escáner apuntaría `folder_id` en cada carpeta y, al ver el mismo `FileID` en otra ruta, la entidad de carpeta vieja cedería su marca a la nueva (retractar y afirmar, I4/I5). Coste: el escáner tiene que recorrer carpetas y no sólo archivos; el código de Windows sólo se puede comprobar aquí que compila (en Linux sirve el inodo, que también se conserva al renombrar).
   - **B · Que la carpeta *sea* su `FileID`**: la entidad de carpeta se reconoce por `FileID` y la ruta pasa a ser una observación, como en los archivos (I8). Es lo más limpio, pero cambia cómo `obrain-link` crea carpetas (hoy por `folder_path`) y toca el árbol, el mapa y las marcas. Cambio grande.
   - **C · Sin `FileID`**: si todos los documentos de una carpeta marcada reaparecen juntos bajo otra ruta, la marca se va con ellos. Funciona en cualquier disco (USB en FAT), pero es una heurística: una carpeta vacía, o una de la que sólo moviste una parte, pierde la marca.
   - En todas: «Sensibles» dejaría de avisar en rojo de una marca huérfana en ese caso, y si algo sale mal la marca se queda en la ruta vieja (lo seguro).
   Luego el ADR (0027), el código y los tests.
5. **Enséñame las preguntas abiertas del maestro (§10) que conviene decidir ahora** y pregúntame si descongelamos algo de la Fase 4:
   - **Regiones del usuario**: agrupar a mano sin que el programa pierda de vista qué es qué. ¿Una región es una entidad con una faceta? ¿Qué pasa si un documento cae en dos? ¿Qué manda al dibujar el mapa?
   - **La Papelera de Windows**: ¿se indexa `$Recycle.Bin`? Con la papelera de obrain (ADR-0025) ya hay un sitio natural para enseñarlo, pero es mirar dentro de una carpeta del sistema.
   - **Un documento dentro de otro** (un PDF que contiene los estados de cuenta que también tienes sueltos): ¿predicado nuevo?, ¿lo detecta M3 comparando hashes de fragmentos?, ¿cómo se enseña?
   - Y la Fase 4: trámites como `Process` y la pregunta de la visa.

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

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` y capturas antes de darla por buena.
- Las decisiones con coste, antes del código: enséñamelas y déjame elegir. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final, recuérdame lo que me toca hacer a mí en Windows (está en `docs/ESTADO.md`).
