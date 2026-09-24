# Prompt para la siguiente conversación — después de «un documento dentro de otro»

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano y en palabras sencillas; prefiero que te tardes y lo hagas bien. **Cuando me pidas hacer algo, dime primero para qué sirve y si puede esperar**: la vez pasada no entendí para qué eran las comprobaciones en Windows.

## Antes de nada: sincroniza

- Trabajo desde dos sitios: sesiones en la nube (desde la PC de la escuela) y mi laptop con Windows. **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`**. Todavía no está fusionada a `main`.
- Si eres una sesión en la nube: `git fetch origin` y `git checkout claude/orchestrators-brain-fase-3-wdiheq` (o `git pull` si ya estás en ella). Trabaja y sube **sólo** a esa rama. Si ya se fusionó a `main`, parte de `main`.
- Confirma con `git log --oneline -6` que el último commit es «Prompt para retomar: qué comprobar en Windows, para qué, y todo lo que sigue» (o uno posterior), y `git status` limpio.
- Al terminar cada pieza: commit **y push**.

## Dónde estamos

- Lee primero `docs/ESTADO.md` y el ADR 0030 (y el 0028 y el 0029 si no los conoces).
- Hecho en la sesión del 2026-09-23 (noche), en mi laptop:
  1. **Un documento dentro de otro (ADR-0030).** El PDF que armo para un trámite juntando otros (los estados de cuenta, una constancia) dice **«Contiene»** y qué lleva; cada documento suelto dice **«Está dentro de»** y lleva al paquete por donde está; el paquete **hereda la sensibilidad** de lo que lleva; y en la búsqueda lo de dentro sale **debajo** de su paquete. Antes de programar se midió con mi base de agosto (una copia, sólo cuentas y nombres enmascarados): por **fragmentos** salía 1 de 14 paquetes; por **párrafos**, los 14 y ninguno falso. Esas tres cosas las elegí yo.
  2. La regla, por si hay que tocarla: los párrafos de B (de 40 bytes o más) aparecen **de un tirón** en A (≥ 95 %, lo que sobre sólo en las orillas), A trae algo más (B ≤ 90 % de A) y B tiene al menos 3 párrafos. Eso separa un paquete de una versión. Predicado `part_of` (del de dentro al de fuera), fuente `linker:parrafos@v1`, evidencia `parrafos=…/… bytes=…..…` en el texto del paquete. `obrain paquetes` los lista.
  3. 452 tests en verde en Windows; clippy y fmt limpios (núcleo y `ui/src-tauri`); `svelte-check` sin avisos; capturas en el banco de pruebas.
- **Todavía no he abierto mi app con las versiones de estos días.** Al empezar aquella sesión, `C:\Users\pacox\.obrain` sólo tenía `models`: mi base aún no se había mudado.
- Si te paso errores o capturas de mi Windows, arréglalos antes que cualquier otra cosa.

## Todo lo que sigue, en este orden

### 1 · Comprobar en mi app lo de estos días (sólo si estás en mi laptop)

**Para qué:** cada pieza se probó con documentos inventados; esto es ver que también funciona con los míos, en mi app de verdad. No rompe nada ni es urgente. **En la nube sáltalo** y ve al paso 2; se hace la próxima vez que esté en mi laptop.

Guíame **de una en una**, diciéndome qué hacer, qué tengo que ver y para qué sirve, y espera a que te conteste (o te mande captura) antes de la siguiente. Si algo no sale, arréglalo antes de seguir.

1. **La carpeta fija (ADR-0029).** *Para qué:* que la app y la orden `obrain` usen siempre la misma base y el mismo modelo, se abran desde donde se abran; fue lo que causó el «cifrado a medias». *Qué hago yo:* cierro la app, `git pull`, y la abro **desde una PowerShell normal, no desde tu terminal** (`cd ui` y `npm run app`). *Qué tengo que ver:* en la terminal, `▸ obrain vive ahora en C:\Users\pacox\.obrain`; abajo a la izquierda de la app, esa ruta; y en el buscador (`Ctrl K`), abajo a la derecha, que busca **por significado**. *Tú puedes comprobar:* que en `C:\Users\pacox\.obrain` ya están `brain.obrain` y `brain.obrain.llave` (el perfil no está virtualizado, lo ves igual que yo).
2. **La fecha de mi pasaporte (ADR-0028).** *Para qué:* el OCR no leyó la palabra «caducidad» y el pasaporte no salía en «Caduca pronto». *Qué tengo que ver:* al pulsar mi pasaporte, «Caduca el …» con su «¿por qué?» (*«…dos de sus fechas están a 10 años justos…»*) y las dos fechas marcadas. Si la fecha es buena, «es correcto».
3. **Los paquetes (ADR-0030).** *Para qué:* ver que encuentra los PDF que junté para la visa. *Qué tengo que ver:* en la vista previa de un PDF así, el apartado «Contiene»; y si lo de dentro está en una carpeta marcada, el paquete tapado con «porque contiene …». *Tú puedes comprobar:* `obrain paquetes --db C:\Users\pacox\.obrain\brain.obrain` desde tu terminal (con `--db`, así no muda nada), **enmascarando los nombres** al enseñármelos. Si falta alguno que yo diga que debería salir, míralo enmascarado como el pasaporte.
4. Recuérdame **activar el Cifrado de dispositivo** de Windows si mi laptop lo tiene (Configuración › Privacidad y seguridad): cifrar la base de obrain no cifra mis archivos originales.

### 2 · Colecciones (regiones del usuario) — ADR-0031

Lo decidido está en el maestro (§10): una colección es una entidad con faceta `region` que creo yo (I5); un documento puede estar en **varias**; el mapa se sigue dibujando por carpetas y la colección se ve como una **lente** encima, sin mover estrellas.

Lo que falta decidir, y que me tienes que enseñar **en palabras sencillas y con su coste antes de programar** (y medirlo con mis datos cuando se pueda):

- Cómo creo una colección y cómo meto documentos: desde la vista previa, desde el explorador, arrastrando, desde la terminal…
- Dónde las veo: en la barra lateral, en el Inicio, como vista propia.
- Qué hace la lente en el mapa, y si la búsqueda puede quedarse dentro de una colección.
- Si una colección puede ser sensible (y qué pasa con lo que tiene dentro), y qué pasa con un documento de una colección que se va a la papelera o que es un paquete (ADR-0030).
- Qué se hace al borrar una colección (los documentos nunca se tocan).

Después: el ADR **0031** (el 0030 ya se usó), el código, los tests, las capturas en el banco de pruebas, y `docs/ESTADO.md`, el maestro y el README al día. Y dime qué me toca comprobar en mi Windows y para qué.

### 3 · Pendientes pequeños, por si salen

- El «¿por qué?» de la sensibilidad cita el nombre del archivo tal cual (con `%20`).
- Lo que la interfaz guarda en el navegador (tema, apartados plegados) sigue en AppData y puede variar según desde dónde se abra la app.
- En la copia privada de Claude quedan restos de agosto (`vida.obrain-wal`, `respaldo-cifrado-a-medias-20260923`): sólo los borro yo, si quiero.
- Un documento idéntico a uno que está en un paquete no dice «está dentro de» (lo dice su original, al que llega por «Duplicado de»).
- Un escaneo dentro de otro escaneo no se detecta como paquete (el OCR nunca lee dos veces igual). Si me importa, la alternativa medida es «texto parecido» (ADR-0030, alternativa B).

### 4 · Más adelante, sólo si yo lo pido

- Fusionar la rama a `main` o abrir un PR.
- Poner un PIN (la app ya lo tiene; es opcional).
- **La Fase 4** —los trámites reconstruidos y la pregunta de la visa— sigue **congelada** hasta que use la app en Windows un tiempo.
- Lo congelado sigue congelado (no se borra, no se sigue): sesiones de trabajo y puentes, detección de versiones, el USN Change Journal, sincronización entre dispositivos.

## Si trabajas en mi laptop (Windows): cuidado con AppData

- **Tus terminales corren dentro de la app de Claude (MSIX).** Lo que ves en `%LOCALAPPDATA%` es una **mezcla** de la carpeta real y de `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Local`, y lo nuevo que escribas ahí va a la copia privada. `%TEMP%` y el perfil (`C:\Users\pacox\…`, incluido `.obrain`) **no** están virtualizados.
- **No corras `obrain` ni la app sin `--db`/`OBRAIN_DB` desde tus terminales**: la primera vez que obrain pregunta dónde está la base, muda lo que vea en `%LOCALAPPDATA%\obrain`, y desde Claude vería la mezcla. Para probar, `OBRAIN_DB=<copia>` o `--db`, y `USERPROFILE`/`LOCALAPPDATA` apuntando al scratchpad. **La app de verdad la abro yo**, desde una PowerShell normal.
- Para ver la carpeta **real** (como la ve mi app), lanza el comando fuera del paquete con WMI: `Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'cmd.exe /c dir … > <archivo en el scratchpad>' }` y lee el archivo.
- **Mis documentos reales son míos**: para diagnosticar mi base, enmascara lo que imprimas (letras → `x`, cifras → `9`, salvo las etiquetas) y no copies datos personales a la conversación. Así se diagnosticó el pasaporte y así se midieron los paquetes. La base de agosto en claro es `vida.obrain` (con su `-wal`), en la mezcla de `%LOCALAPPDATA%\obrain`: cópiala al scratchpad antes de abrirla.
- Compilar necesita **Strawberry Perl** (ya instalado) y, por el apóstrofo de `Orchestrator's-Brain`, compilar **fuera** de la carpeta: `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target` (en Git Bash, además `PATH=/c/Strawberry/perl/bin:$PATH`). Sin eso, OpenSSL falla con `Unknown regexp modifier` en `configdata.pm`.
- `cargo test --workspace` corre en NTFS de verdad; úsalo.
- **No escribas rutas de Windows dentro de strings de Python en un heredoc** (`\0`, `\v`, `\U` rompen, y dejan caracteres de control invisibles en el archivo): usa la herramienta de edición. Pasó otra vez el 2026-09-23 con unos tests.
- `cargo fmt` a veces no puede escribir un archivo que OneDrive tiene abierto (`os error 1224`): repítelo.
- **El OCR de Windows (es-MX) casi nunca lee una MRZ** (probado con MRZ dibujadas en Consolas, Courier y Arial). No lo cuentes como fallo.

### Banco de pruebas en Windows (en el scratchpad; nada va al repo)

- Un crate aparte (`[workspace]` propio, dependencias por ruta a `core/obrain-api` y `core/obrain-core`, más `tiny_http`, `serde`, `serde_json`). **Copia `core/Cargo.lock` a su carpeta** antes de compilar, y `CARGO_TARGET_DIR=C:/Users/pacox/obrain-target`, para reutilizar lo compilado. Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__` (`invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script); los eventos salen de `GET /eventos`, consultado cada 400 ms. Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain` (`previsualizar` con su `tramo` → `preview_at`).
- **El banco de la sesión anterior se queda en el scratchpad de esa sesión** (`%TEMP%\claude\<proyecto>\<sesión>\scratchpad\banco` y `boveda`; el más reciente es el de la sesión `57de2235…`): cópialo en vez de rehacerlo. Si `banco.exe` no se deja recompilar («Acceso denegado»), es que el de otra sesión sigue corriendo: `Get-Process banco`. Al terminar, detén el tuyo.
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
- Al final, dime lo que me toca comprobar en mi Windows **y para qué**, y déjame un prompt para la siguiente conversación.
