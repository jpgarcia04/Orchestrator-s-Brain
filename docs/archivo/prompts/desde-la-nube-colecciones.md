# Prompt para una sesión en la nube, sin mi laptop — las colecciones

Copiar y pegar tal cual al empezar una conversación nueva **en la nube** (Claude Code en la web, desde la PC de la escuela o el celular), cuando **no** tengo mi laptop a mano.

Si estoy en mi laptop, el prompt es otro: [`despues-de-un-documento-dentro-de-otro.md`](despues-de-un-documento-dentro-de-otro.md). Ése empieza por comprobar cosas en mi app, y eso sólo se puede hacer ahí.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano y en palabras sencillas; prefiero que te tardes y lo hagas bien. **Cuando me pidas algo, dime primero para qué sirve.**

## Hoy no tengo mi laptop

- Estás en la **nube**. Yo no puedo abrir mi app ni correr nada en Windows, así que **no me pidas comprobaciones en mi app**. Las comprobaciones que tengo pendientes (la carpeta fija, la fecha del pasaporte, los paquetes) se quedan para cuando esté en la laptop; no las toques ni las des por hechas.
- Mis documentos y mi base de verdad **no están aquí**. Todo se prueba con la **bóveda ficticia** del banco de pruebas. Donde una decisión se mediría «con mis datos», dime qué medirías y lo apuntamos para la laptop.
- Puedo contestarte preguntas desde el celular: si una decisión tiene coste, enséñamela con opciones cortas.

## Antes de nada: sincroniza

- **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`** (todavía no está en `main`). La nube clona el repo de cero y quizá te deje en otra rama: haz `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq`, y trabaja y sube **sólo** ahí.
- Confirma con `git log --oneline -6` que el último commit es «Prompt para una sesión en la nube: las colecciones» (o uno posterior), y `git status` limpio.
- **Al terminar cada pieza: commit y push.** La sesión en la nube se borra al cerrarse; lo que no se sube, se pierde. Antes de despedirte, comprueba con `git status -sb` que no queda nada sin subir.

## Dónde estamos

- Lee primero `docs/ESTADO.md`, el maestro (`docs/PROJECT-MASTER.md`, §10: la decisión de las colecciones) y los ADR 0030 (un documento dentro de otro), 0025 (papelera) y 0020 (sensibilidad por carpeta).
- La Fase 3 está hecha y probada en mi Windows (452 tests en verde allí). Lo último fue **un documento dentro de otro** (ADR-0030). La Fase 4 sigue **congelada**.

## Lo que se puede hacer hoy, en este orden

### 1 · Las colecciones (regiones del usuario) — ADR-0031

**Para qué:** poder agrupar mis documentos a mi gusto —«Trámite de la visa», «Todo lo del piso»— sin moverlos de carpeta, y que obrain no pierda de vista qué es qué.

Lo que ya decidí (maestro, §10): una colección es una entidad con faceta `region` que creo yo (I5); un documento puede estar en **varias**; el mapa se sigue dibujando por carpetas y la colección se ve como una **lente** encima, sin mover estrellas.

Lo que falta decidir. **Enséñamelo en palabras sencillas y con su coste antes de programar**, en pocas preguntas (lo voy a contestar desde el celular):

- Cómo creo una colección y cómo meto documentos: desde la vista previa, desde el explorador (selección de varios), arrastrando, desde la terminal…
- Dónde las veo: en la barra lateral, en el Inicio, como vista propia.
- Qué hace la lente en el mapa, y si la búsqueda puede quedarse dentro de una colección.
- Si una colección puede ser sensible (y qué pasa con lo que tiene dentro); qué pasa con un documento de una colección que se va a la papelera (ADR-0025) o que es un paquete (ADR-0030).
- Qué se hace al borrar una colección (los documentos nunca se tocan).

Después: el ADR **0031**, el código, los tests, las capturas en el banco de pruebas, y `docs/ESTADO.md`, el maestro y el README al día. Apunta en ESTADO lo que me tocará comprobar en mi app, **y para qué**.

### 2 · Pendientes pequeños que no necesitan mi laptop

- El «¿por qué?» de la sensibilidad cita el nombre del archivo tal cual, con `%20` y demás (nombres de descargas). Que se lea limpio.
- Un documento idéntico a uno que está dentro de un paquete no dice «Está dentro de» (lo dice su original, al que llega por «Duplicado de»). Mira si conviene que lo diga él también y, si tiene coste, pregúntame.

(Los otros pendientes —lo que la interfaz guarda en AppData, los restos de agosto en la copia privada de Claude, los escaneos dentro de escaneos— necesitan mi laptop o mis datos: déjalos.)

### 3 · Más adelante, sólo si yo lo pido

- Fusionar la rama a `main` o abrir un PR.
- **La Fase 4** sigue congelada hasta que use la app un tiempo. Lo congelado sigue congelado: sesiones de trabajo y puentes, detección de versiones, el USN Change Journal, sincronización entre dispositivos.

## Entorno en la nube

- **ONNX Runtime**: la descarga de `cdn.pyke.io` está bloqueada; no la esquives. `pip download onnxruntime --no-deps`, descomprime el wheel (`python3 -m zipfile -e`), copia `libonnxruntime.so.1.x` a un `lib/` con los enlaces `libonnxruntime.so.1` y `libonnxruntime.so`, y compila con `ORT_LIB_LOCATION=<dir>/lib ORT_PREFER_DYNAMIC_LINK=1 LD_LIBRARY_PATH=<dir>/lib`. Cada llamada a la terminal empieza de cero: pon las tres variables en un `env.sh` en el scratchpad y haz `source` en cada una.
- **Tauri en Linux**: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`. En `ui/`, `npm install`. La primera compilación del núcleo tarda unos minutos: lánzala en segundo plano mientras lees.
- **Playwright** está en `/opt/node22/lib/node_modules` (`NODE_PATH`), con Chromium en `/opt/pw-browsers`; `chromium.launch()` sin más lo encuentra. Tamaño de ventana 1440×900.
- **Código de Windows**: `rustup target add x86_64-pc-windows-msvc` y `cargo clippy -p obrain-keys -p obrain-ocr --target x86_64-pc-windows-msvc --all-targets -- -D warnings`. El núcleo entero no se puede comprobar así (SQLCipher necesita el compilador de C de Microsoft). Si tocas algo de Windows, dilo: se prueba cuando yo esté en la laptop.
- **Tests que también corren en Windows**: una ruta de prueba se arma con `join` trozo a trozo (el escáner guarda `\`), y no cuentes con empates de hora de modificación (NTFS guarda 100 ns). En Linux los inodos se reutilizan: un archivo nuevo puede heredar el de uno borrado (en NTFS no pasa).
- **Procesos**: `pgrep -f`/`pkill -f` se encuentran a sí mismos; usa `pgrep -x`/`pkill -x`.
- **No escribas rutas de Windows dentro de strings de Python en un heredoc**: usa la herramienta de edición. Tras `cargo fmt`, un reemplazo de texto por script puede no casar: comprueba que se aplicó.

### Banco de pruebas (en el scratchpad; nada va al repo; hay que rehacerlo en cada sesión)

- Un crate aparte con `[workspace]` propio, dependencias por ruta a `core/obrain-api`, `core/obrain-core` y `core/obrain-scan`, más `tiny_http`, `serde` y `serde_json`. **Copia `core/Cargo.lock` a su carpeta** y compila con `CARGO_TARGET_DIR=<repo>/core/target` para reutilizar lo compilado.
- Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__`: `invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script. Los eventos (`indexando`, `indexado`) salen de `GET /eventos`, que la página consulta cada 400 ms.
- Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain` (mira ahí los que haya, incluidos `previsualizar` con su tramo, `papelera`, `olvidar` y lo de los paquetes). `actualizar` reindexa en un hilo con **otra conexión** (`seguridad::abrir`), como la app.
- La bóveda ficticia: `.txt` con datos inventados (un pasaporte, una TIE, una INE, una póliza con «Vigencia: del 1 de septiembre de 2026 al 31 de agosto de 2027», un contrato que «vence el 30 de septiembre de 2026», dos meses de movimientos y un «Expediente consulado» que los junta, para los paquetes). Para fotos: Pillow en un venv y Tesseract (`apt-get install tesseract-ocr tesseract-ocr-spa`) sólo en el puente.
- La base: `obrain indexar <bóveda> --db <scratchpad>/db/brain.obrain --sin-modelo`, `obrain clave --guardada --db …` y `obrain sensible "<bóveda>/01 Identidad" --db …`.

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` (y `npx svelte-check`) y capturas antes de darla por buena.
- Las decisiones con coste, antes del código: enséñamelas **en palabras sencillas** y déjame elegir. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final: dime qué me tocará comprobar en mi app **y para qué**, y deja al día **los dos prompts** —el de la laptop y éste de la nube— con lo que siga.
