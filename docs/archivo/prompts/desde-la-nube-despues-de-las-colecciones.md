# Prompt para una sesión en la nube, sin mi laptop — después de las colecciones

Copiar y pegar tal cual al empezar una conversación nueva **en la nube** (Claude Code en la web, desde la PC de la escuela o el celular), cuando **no** tengo mi laptop a mano.

Si estoy en mi laptop, el prompt es otro: [`despues-de-las-colecciones.md`](despues-de-las-colecciones.md). Ése empieza por comprobar cosas en mi app, y eso sólo se puede hacer ahí.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano y en palabras sencillas; prefiero que te tardes y lo hagas bien. **Cuando me pidas algo, dime primero para qué sirve.**

## Hoy no tengo mi laptop

- Estás en la **nube**. Yo no puedo abrir mi app ni correr nada en Windows, así que **no me pidas comprobaciones en mi app**. Las que tengo pendientes (los tests en Windows, la carpeta fija, la fecha del pasaporte, los paquetes y sus copias, mis colecciones) se quedan para cuando esté en la laptop; no las toques ni las des por hechas.
- Mis documentos y mi base de verdad **no están aquí**. Todo se prueba con la **bóveda ficticia** del banco de pruebas. Donde una decisión se mediría «con mis datos», dime qué medirías y lo apuntamos para la laptop.
- Puedo contestarte preguntas desde el celular: si una decisión tiene coste, enséñamela con opciones cortas.

## Antes de nada: sincroniza

- **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`** (todavía no está en `main`). La nube clona el repo de cero y quizá te deje en otra rama: haz `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq`, y trabaja y sube **sólo** ahí.
- Confirma con `git log --oneline -6` que el último commit es «Prompts para retomar después de las colecciones» (o uno posterior), y `git status` limpio.
- **Al terminar cada pieza: commit y push.** La sesión en la nube se borra al cerrarse; lo que no se sube, se pierde. Antes de despedirte, comprueba con `git status -sb` que no queda nada sin subir.

## Dónde estamos

- Lee primero `docs/ESTADO.md`, el maestro (`docs/PROJECT-MASTER.md`) y los ADR 0031 (tus colecciones) y 0032 (las copias también están dentro).
- Lo último (2026-09-24, en la nube): **las colecciones** —juntar a mano sin mover nada, meter desde la vista previa o varias a la vez, verlas en la barra lateral y en su vista, buscar dentro, una lente en el mapa—; **las copias también están dentro** —el paquete ya no sale sin tapar cuando la copia protegida no es la más vieja—; y el «¿por qué?» sin `%20`. 467 tests en verde en Linux; **los 15 nuevos, sin correr en Windows**. La Fase 4 sigue **congelada**.

## Lo que se puede hacer hoy, en este orden

### 1 · Si te traigo algo de la laptop, eso primero

Si te paso errores, capturas o lo que salió al comprobar en mi app (los tests en Windows, las colecciones, los paquetes), arréglalo antes que cualquier otra cosa, con un test que lo reproduzca.

### 2 · Pendientes pequeños que no necesitan mi laptop

- En la vista previa, los tres botones de abajo («Abrir», «En su carpeta», «Ver en el mapa») no caben en una línea y el tercero baja solo a otra. Que se repartan bien. Es sólo de aspecto; enséñame la captura antes y después.
- En el mapa de una colección, un documento que está en una carpeta pequeña (que el cielo no abre como estrella propia) enciende la estrella «El resto de …». Es correcto, pero puede confundir: mira si conviene que la leyenda o el título lo digan y, si tiene coste, pregúntame.

(Los otros pendientes —lo que la interfaz guarda en AppData, los restos de agosto en la copia privada de Claude, los escaneos dentro de escaneos— necesitan mi laptop o mis datos: déjalos.)

### 3 · Más adelante, sólo si yo lo pido

- Arrastrar documentos a una colección (hay que cambiar `dragDropEnabled` de Tauri y sólo se prueba en Windows), o las colecciones en el Inicio.
- Fusionar la rama a `main` o abrir un PR.
- **La Fase 4** sigue congelada hasta que use la app un tiempo. Lo congelado sigue congelado: sesiones de trabajo y puentes, detección de versiones, el USN Change Journal, sincronización entre dispositivos.

## Entorno en la nube

- **ONNX Runtime**: la descarga de `cdn.pyke.io` está bloqueada; no la esquives. `pip download onnxruntime --no-deps`, descomprime el wheel (`python3 -m zipfile -e`), copia `libonnxruntime.so.1.x` a un `lib/` con los enlaces `libonnxruntime.so.1` y `libonnxruntime.so`, y compila con `ORT_LIB_LOCATION=<dir>/lib ORT_PREFER_DYNAMIC_LINK=1 LD_LIBRARY_PATH=<dir>/lib`. Cada llamada a la terminal empieza de cero: pon las tres variables en un `env.sh` en el scratchpad y haz `source` en cada una.
- **Tauri en Linux**: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`. En `ui/`, `npm install`. La primera compilación del núcleo tarda unos minutos, y la de `ui/src-tauri` otros dos o tres: lánzalas en segundo plano mientras lees. `ui/src-tauri` necesita que exista `ui/dist` (haz antes `npx vite build`).
- **Playwright** está en `/opt/node22/lib/node_modules`: corre los scripts con `NODE_PATH=/opt/node22/lib/node_modules node <script>.js`; `chromium.launch()` sin más encuentra Chromium. Tamaño de ventana 1440×900.
- **Código de Windows**: `rustup target add x86_64-pc-windows-msvc` y `cargo clippy -p obrain-keys -p obrain-ocr --target x86_64-pc-windows-msvc --all-targets -- -D warnings`. El núcleo entero no se puede comprobar así (SQLCipher necesita el compilador de C de Microsoft). Si tocas algo de Windows, dilo: se prueba cuando yo esté en la laptop.
- **Tests que también corren en Windows**: una ruta de prueba se arma con `join` trozo a trozo (el escáner guarda `\`), y no cuentes con empates de hora de modificación (NTFS guarda 100 ns). En Linux los inodos se reutilizan: un archivo nuevo puede heredar el de uno borrado (en NTFS no pasa).
- **Procesos**: `pgrep -f`/`pkill -f` se encuentran a sí mismos; usa `pgrep -x`/`pkill -x` (p. ej. `pkill -x banco` antes de recompilar el banco).
- **No escribas rutas de Windows dentro de strings de Python en un heredoc**: usa la herramienta de edición. Tras `cargo fmt`, un reemplazo de texto por script puede no casar: comprueba que se aplicó.
- **Nombres que activan la sensibilidad**: al inventar documentos para un experimento, «visa», «pasaporte» o «póliza» en el nombre ya los hacen sensibles por sí solos. Si mides algo de sensibilidad, usa nombres y textos neutros o el resultado sale contaminado (pasó el 2026-09-24).

### Banco de pruebas (en el scratchpad; nada va al repo; hay que rehacerlo en cada sesión)

- Un crate aparte con `[workspace]` propio, dependencias por ruta a `core/obrain-api` y `core/obrain-core`, más `tiny_http`, `serde` y `serde_json`. **Copia `core/Cargo.lock` a su carpeta** y compila con `CARGO_TARGET_DIR=<repo>/core/target` para reutilizar lo compilado.
- Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__`: `invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script. Los eventos (`indexando`, `indexado`) salen de `GET /eventos`, que la página consulta cada 400 ms.
- Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain` (mira ahí los que haya): `previsualizar` con su tramo (`preview_at`), `papelera`, `olvidar`, lo de los paquetes, **las colecciones** (`colecciones`, `coleccion`, `crear_coleccion`, `renombrar_coleccion`, `borrar_coleccion`, `meter_en_coleccion`, `sacar_de_coleccion`) y `vistazo` con su `coleccion` (`glance_in`). `actualizar` reindexa en un hilo con **otra conexión** (`seguridad::abrir`), como la app. Mientras el banco corre, `obrain indexar --db …` desde la terminal también funciona (la base está en WAL); tras recompilar el núcleo, para y vuelve a arrancar el banco.
- La bóveda ficticia: `.txt` con datos inventados (un pasaporte, una TIE, una INE, una póliza con «Vigencia: del 1 de septiembre de 2026 al 31 de agosto de 2027», un contrato que «vence el 30 de septiembre de 2026», una matrícula, una cita de extranjería, dos meses de movimientos y un «Expediente consulado» que los junta, una copia de uno de los meses en `01 Identidad` para las copias, y en `07 Descargas` un `Pasaporte%20escaneado%20copia.txt` para los nombres de descargas). Para fotos: Pillow en un venv y Tesseract (`apt-get install tesseract-ocr tesseract-ocr-spa`) sólo en el puente.
- La base: `obrain indexar <bóveda> --db <scratchpad>/db/brain.obrain --sin-modelo`, `obrain clave --guardada --db …` y `obrain sensible "<bóveda>/01 Identidad" --db …`. Colecciones de ejemplo: `obrain coleccion "Trámite de la visa" --crear --db …` y `--meter <archivos>…`.

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` (que ya corre `svelte-check`) y capturas antes de darla por buena. **Mira las capturas de verdad**: el 2026-09-24 destaparon cuatro fallos que los tests no veían.
- Las decisiones con coste, antes del código: enséñamelas **en palabras sencillas** y déjame elegir. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final: dime qué me tocará comprobar en mi app **y para qué**, y deja al día **los dos prompts** —el de la laptop y éste de la nube— con lo que siga.
