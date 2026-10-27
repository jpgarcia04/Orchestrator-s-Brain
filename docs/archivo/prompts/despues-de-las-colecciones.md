# Prompt para la siguiente conversación en la laptop — después de las colecciones

Copiar y pegar tal cual al empezar la conversación nueva **en mi laptop**.

Si estoy en la nube, sin mi laptop, el prompt es otro: [`desde-la-nube-despues-de-las-colecciones.md`](desde-la-nube-despues-de-las-colecciones.md).

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano y en palabras sencillas; prefiero que te tardes y lo hagas bien. **Cuando me pidas hacer algo, dime primero para qué sirve.**

## Antes de nada: sincroniza

- Ahorita **todo lo hago en mi laptop con Windows**: tú corres aquí, en la app de Claude, y mi app de obrain también está aquí.
- **Todo vive en GitHub**, en la rama **`claude/orchestrators-brain-fase-3-wdiheq`** (todavía no está fusionada a `main`). Haz `git fetch origin` y `git pull` en esa rama, y confirma con `git log --oneline -6` que el último commit es «Estado y prompts tras arrastrar a una colección» (o uno posterior) y `git status` limpio.
- Al terminar cada pieza: commit **y push** a esa rama.

## Dónde estamos

- Lee primero `docs/ESTADO.md` y los ADR 0031 (tus colecciones), 0032 (las copias también están dentro) y 0033 (arrastrar a una colección). Si no conoces el 0030 (un documento dentro de otro), léelo también.
- Hecho en la sesión en la nube del 2026-09-24 (sin mi laptop, con datos inventados):
  1. **Tus colecciones (ADR-0031).** Junto a mano lo que va junto aunque viva en carpetas distintas, sin mover nada. Lo elegí yo desde el celular, con su coste: se meten **desde la vista previa** («Colecciones») y **varios a la vez** (`Ctrl`+clic, `Mayús`+clic, `Ctrl A`, y la barra de abajo); se ven en la **barra lateral** y en una **vista propia**; con una colección abierta, **se busca sólo dentro** (el filtro va antes de cortar la lista); **una colección sólo agrupa** (no es sensible ni protege); un **paquete no mete** lo que lleva; **sólo documentos**, no carpetas en vivo. Lo de la papelera sale de la colección y vuelve solo; borrar una colección no toca ningún documento. En el mapa, «Ver en el mapa» enciende las estrellas de sus documentos sin mover ninguna, y la lente de un documento enseña sus colecciones. En la terminal, `obrain colecciones` y `obrain coleccion "<nombre>"` (con `--crear`, `--meter`, `--sacar`, `--renombrar`, `--borrar`). Por dentro: faceta `region`, predicados `collection_name` y `member_of`, todo `user`.
  2. **Las copias también están dentro (ADR-0032).** Al mirar el pendiente de la copia que no decía «Está dentro de» salió un hueco: el paquete sólo heredaba la sensibilidad de la copia **más vieja**. Con mi pasaporte en `01 Identidad` y una copia más vieja en otra carpeta, el PDF de la visa salía **sin tapar** (comprobado en el banco de pruebas). Elegí que **cuenten todas las copias**: cada una dice «Está dentro de», el paquete se tapa si cualquiera está protegida, y «Contiene» las junta en una fila («y 1 copia»).
  3. El «¿por qué?» de la sensibilidad ya cita el nombre **sin `%20`**.
  4. 467 tests en verde **en Linux**; clippy y fmt limpios (núcleo y `ui/src-tauri`); `svelte-check` sin avisos; capturas en el banco de pruebas. **Los 15 tests nuevos no se han corrido en Windows.**
- Hecho en otra sesión en la nube, el 2026-09-25 (sin mi laptop):
  1. **Los botones de la vista previa.** «Abrir», «En su carpeta» y «Ver en el mapa» no cabían en una fila y el tercero bajaba solo. Ahora «Abrir» ocupa la fila entera y los otros dos comparten la de abajo, del mismo ancho. Sólo CSS, en `VistaPrevia.svelte`; los tests siguen siendo 467.
  2. **Lo decidí yo:** los ajustes que dependen de cómo quede mi bóveda se hacen cuando sea la definitiva, con todos mis documentos reales, no durante el desarrollo. El primero: en el mapa de una colección, un documento de una carpeta pequeña enciende «El resto de …» sin decir cuál es. **No lo toques** hasta que yo lo pida.
  3. **Arrastrar a una colección (ADR-0033).** Lo elegí yo desde el celular: se arrastra **desde una carpeta o una colección** (si agarro uno de los elegidos, van todos); se suelta en una colección de la **barra lateral** o en **«Nueva colección»**, que sale mientras arrastro y pide el nombre al soltar; desde una carpeta **añade**, de una colección a otra **mueve**, y con `Ctrl` se queda en las dos. Junto al ratón dice qué va a pasar antes de soltar; `Esc` cancela. **En mis carpetas de Windows no cambia nada** (pregunté eso y la respuesta es no: una colección es como una playlist). Se hizo **con eventos de puntero, como el mapa, sin tocar `dragDropEnabled` de Tauri**. En el núcleo, `Brain::move_between_collections` mueve en una sola transacción. 469 tests en verde en Linux; **los 2 nuevos, sin correr en Windows**.
  4. **Una idea mía, por evaluar:** que la app sea como el Explorador de Windows pero sólo con lo importante de mi vida, y que **yo elija qué entra y qué no**. Faltan dos piezas: **quitar una carpeta** de la app y **esconder algo de dentro** (una subcarpeta o un archivo). Está en el §10 del maestro.
- **Todavía no he abierto mi app con las versiones de estos días.**
- Si te paso errores o capturas, arréglalos antes que cualquier otra cosa.

## Todo lo que sigue, en este orden

### 1 · Correr los tests en Windows

**Para qué:** los 17 tests nuevos sólo se han corrido en Linux; en NTFS los inodos y las horas se portan distinto, y lo que pasa en Linux no siempre pasa aquí. Es rápido y no toca nada mío.

`cargo test --workspace` (con `CARGO_TARGET_DIR=$env:USERPROFILE/obrain-target` y Perl en el `PATH`, abajo). Si algo falla, arréglalo antes de seguir.

### 2 · Comprobar en mi app lo de estos días

**Para qué:** cada pieza se probó con documentos inventados; esto es ver que también funciona con los míos, en mi app de verdad, que es lo único que tú no puedes hacer por mí. No rompe nada. Va antes de construir nada nuevo porque, si algo falla, hay que arreglarlo primero.

Guíame **de una en una**: qué hago, qué tengo que ver y para qué sirve. Espera a que te conteste (o te mande captura) antes de pasar a la siguiente. Si algo no sale, arréglalo antes de seguir.

1. **La carpeta fija (ADR-0029).** *Para qué:* que la app y la orden `obrain` usen siempre la misma base y el mismo modelo, se abran desde donde se abran; fue lo que causó el «cifrado a medias». *Qué hago yo:* cierro la app y la abro **desde una PowerShell normal, no desde tu terminal** (`cd ui` y `npm run app`). *Qué tengo que ver:* en la PowerShell, `▸ obrain vive ahora en %USERPROFILE%\.obrain`; abajo a la izquierda de la app, esa ruta; y en el buscador (`Ctrl K`), abajo a la derecha, que busca **por significado**. *Tú puedes comprobar:* que en `%USERPROFILE%\.obrain` ya están `brain.obrain` y `brain.obrain.llave`.
2. **La fecha de mi pasaporte (ADR-0028).** *Para qué:* el OCR no leyó «caducidad» y el pasaporte no salía en «Caduca pronto». *Qué tengo que ver:* al pulsar mi pasaporte, «Caduca el …» con su «¿por qué?» y las dos fechas marcadas. Si la fecha es buena, «es correcto».
3. **Los paquetes y sus copias (ADR-0030 y 0032).** *Para qué:* que encuentre los PDF que junté para la visa y que se tapen si llevan algo protegido, **aunque tenga otra copia** de eso en otra carpeta. *Qué tengo que ver:* en la vista previa de un PDF así, «Contiene», con **una sola fila por documento** (y «· y 1 copia» si tengo copias); si algo de dentro está en una carpeta marcada, el paquete tapado con «porque contiene …». *Tú puedes comprobar:* `obrain paquetes --db %USERPROFILE%\.obrain\brain.obrain` desde tu terminal (con `--db`, así no muda nada), **enmascarando los nombres** al enseñármelos. Mira también si alguno de mis 14 paquetes de agosto cambia de «normal» a «sensible» por una copia: sería el hueco del ADR-0032 cerrándose, y quiero saber cuál (enmascarado).
4. **Mis colecciones (ADR-0031).** *Para qué:* juntar lo de un trámite sin mover nada y que obrain no lo pierda. *Qué hago yo:* «Colecciones» › «Nueva colección» › «Trámite de la visa»; en la vista previa de mi pasaporte, «Colecciones» y la marco; en la carpeta de mis estados de cuenta, `Ctrl`+clic en dos y «Añadir a una colección» en la barra de abajo. *Qué tengo que ver:* la colección con los tres, cada uno con su carpeta de verdad; «Buscar aquí» sólo encuentra lo de dentro; «Ver en el mapa» enciende estrellas **sin moverlas**; si renombro uno en el Explorador de Windows, a los pocos segundos sigue en la colección. *Lo único que es de Windows y no se pudo probar en la nube:* que WebView2 no se quede `Ctrl`+clic ni `Ctrl A`, y que la barra de abajo no tape nada en mi pantalla.
5. **Los botones de la vista previa.** *Para qué:* que ninguno se quede descolgado. *Qué tengo que ver:* al pulsar cualquier documento, abajo, «Abrir» en toda la fila y «En su carpeta» y «Ver en el mapa» debajo, del mismo ancho, sin que ninguno baje solo.
6. **Arrastrar a una colección (ADR-0033).** *Para qué:* juntar un trámite agarrando y soltando, y ver que el ratón y el touchpad de mi laptop hacen lo mismo que en las pruebas. *Qué hago yo:* en una carpeta, agarro un documento y lo llevo, sin soltar, a una colección de la barra lateral; luego elijo dos con `Ctrl`+clic y los arrastro a «Nueva colección» (sale abajo mientras arrastro), le pongo nombre y `Enter`; luego, dentro de una colección, arrastro uno a otra, primero con `Ctrl` pulsado y luego sin él. *Qué tengo que ver:* junto al ratón, *«Añadir a …»*, *«Mover a …»* o *«Ya está en …»*, y la colección iluminada; al soltar, el aviso; al mover sin `Ctrl`, que sale de la primera y que **en el Explorador de Windows el archivo sigue en su carpeta**. Que un clic siga abriendo la vista previa y el doble clic el archivo. *Lo único que no se pudo ver en la nube:* que el touchpad no confunda un clic con un arrastre (se necesita moverse 5 píxeles).
7. Recuérdame **activar el Cifrado de dispositivo** de Windows si mi laptop lo tiene (Configuración › Privacidad y seguridad): cifrar la base de obrain no cifra mis archivos originales.

### 3 · Evaluar conmigo: elegir qué entra en la app y qué no

**Para qué:** que la app sea mi Explorador de Windows pero sólo con lo importante de mi vida, eligiendo yo qué se ve. Lee la entrada del §10 del maestro. **No programes nada todavía**: enséñame primero, en palabras sencillas y con su coste, las dos piezas —**quitar una carpeta** de la app y **esconder algo de dentro** de una carpeta elegida— y qué pasa con lo que ya se leyó (¿a la papelera?, ¿se olvida?). Aquí sí se puede **medir con mis datos** (enmascarado): qué subcarpetas de mi bóveda querría esconder y cuánto de la base es suyo. Luego el ADR y el código.

### 4 · Pendientes pequeños, por si salen

- Lo que la interfaz guarda en el navegador (tema, apartados plegados) sigue en AppData y puede variar según desde dónde se abra la app.
- En la copia privada de Claude quedan restos de agosto (`vida.obrain-wal`, `respaldo-cifrado-a-medias-20260923`): sólo los borro yo, si quiero.
- **Esperan a mi bóveda definitiva** (lo decidí el 2026-09-25; sólo si yo lo pido): los ajustes que dependen de cómo quede mi bóveda. Por ahora uno: en el mapa de una colección, un documento de una carpeta pequeña enciende «El resto de …» sin decir cuál es; se me ofreció que la estrella nombrara esas carpetas.
- Un escaneo dentro de otro escaneo no se detecta como paquete (el OCR nunca lee dos veces igual). Si me importa, la alternativa medida es «texto parecido» (ADR-0030, alternativa B).
- Si con el uso me hace falta: las colecciones **en el Inicio** (dije que no). Sólo si yo lo pido.

### 5 · Más adelante, sólo si yo lo pido

- Fusionar la rama a `main` o abrir un PR.
- Poner un PIN (la app ya lo tiene; es opcional).
- **La Fase 4** —los trámites reconstruidos y la pregunta de la visa— sigue **congelada** hasta que use la app un tiempo.
- Lo congelado sigue congelado (no se borra, no se sigue): sesiones de trabajo y puentes, detección de versiones, el USN Change Journal, sincronización entre dispositivos.

## En mi laptop: cuidado con AppData

- **Tus terminales corren dentro de la app de Claude (MSIX).** Lo que ves en `%LOCALAPPDATA%` es una **mezcla** de la carpeta real y de `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Local`, y lo nuevo que escribas ahí va a la copia privada. `%TEMP%` y el perfil (`%USERPROFILE%\…`, incluido `.obrain`) **no** están virtualizados.
- **No corras `obrain` ni la app sin `--db`/`OBRAIN_DB` desde tus terminales**: la primera vez que obrain pregunta dónde está la base, muda lo que vea en `%LOCALAPPDATA%\obrain`, y desde Claude vería la mezcla. Para probar, `OBRAIN_DB=<copia>` o `--db`, y `USERPROFILE`/`LOCALAPPDATA` apuntando al scratchpad. **La app de verdad la abro yo**, desde una PowerShell normal.
- Para ver la carpeta **real** (como la ve mi app), lanza el comando fuera del paquete con WMI: `Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'cmd.exe /c dir … > <archivo en el scratchpad>' }` y lee el archivo.
- **Mis documentos reales son míos**: para diagnosticar mi base, enmascara lo que imprimas (letras → `x`, cifras → `9`, salvo las etiquetas) y no copies datos personales a la conversación. La base de agosto en claro es `vida.obrain` (con su `-wal`), en la mezcla de `%LOCALAPPDATA%\obrain`: cópiala al scratchpad antes de abrirla.
- Compilar necesita **Strawberry Perl** (ya instalado) y, por el apóstrofo de `Orchestrator's-Brain`, compilar **fuera** de la carpeta: `CARGO_TARGET_DIR=$env:USERPROFILE/obrain-target` (en Git Bash, además `PATH=/c/Strawberry/perl/bin:$PATH`). Sin eso, OpenSSL falla con `Unknown regexp modifier` en `configdata.pm`.
- En los tests, una ruta se arma con `join` trozo a trozo, y no cuentes con empates de hora de modificación (NTFS guarda 100 ns).
- **No escribas rutas de Windows dentro de strings de Python en un heredoc** (`\0`, `\v`, `\U` rompen): usa la herramienta de edición.
- `cargo fmt` a veces no puede escribir un archivo que OneDrive tiene abierto (`os error 1224`): repítelo.
- **El OCR de Windows (es-MX) casi nunca lee una MRZ.** No lo cuentes como fallo.

### Banco de pruebas (en el scratchpad; nada va al repo)

- Un crate aparte (`[workspace]` propio, dependencias por ruta a `core/obrain-api` y `core/obrain-core`, más `tiny_http`, `serde`, `serde_json`). **Copia `core/Cargo.lock` a su carpeta** antes de compilar, y `CARGO_TARGET_DIR=$env:USERPROFILE/obrain-target`. Sirve `ui/dist` en `127.0.0.1:8765` e inyecta en `<head>` un script que define `window.__TAURI_INTERNALS__` (`invoke` → `POST /invoke {cmd, args}`, `transformCallback`, y `plugin:event|listen` guardado en el propio script); los eventos salen de `GET /eventos`, consultado cada 400 ms. Los comandos son los de `ui/src-tauri/src/main.rs` llamando a `Brain`, **ahora también los de las colecciones** (`colecciones`, `coleccion`, `crear_coleccion`, `renombrar_coleccion`, `borrar_coleccion`, `meter_en_coleccion`, `sacar_de_coleccion`, y `mover_entre_colecciones` para el arrastre) y `vistazo` con su `coleccion`.
- El banco de una sesión anterior de la laptop se queda en el scratchpad de esa sesión (`%TEMP%\claude\<proyecto>\<sesión>\scratchpad\banco`): cópialo y añádele los comandos nuevos, en vez de rehacerlo. Si `banco.exe` no se deja recompilar («Acceso denegado»), es que otro sigue corriendo: `Get-Process banco`. Al terminar, detén el tuyo.
- La bóveda ficticia: `.txt` con datos inventados (un «pasaporte» con la forma exacta del OCR del mío, una INE, una TIE, una póliza, un contrato, dos meses de movimientos y un «Expediente consulado» que los junta, y una copia de uno de los meses en `01 Identidad` para las copias), imágenes dibujadas con `System.Drawing` para el OCR. La base: `obrain indexar <bóveda> --db <scratchpad>\db\brain.obrain --sin-modelo`, `obrain clave --guardada --db …` y `obrain sensible "<bóveda>\01 Identidad" --db …`. Las colecciones de ejemplo, con `obrain coleccion … --crear` y `--meter`.
- **Capturas en el navegador integrado:** con tamaños emulados se cuelgan. Usa el tamaño del panel y `document.documentElement.style.zoom = 0.68` (se pierde al recargar).

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` y capturas antes de darla por buena.
- Las decisiones con coste, antes del código: enséñamelas **en palabras sencillas** y déjame elegir. **Mídelas con mis datos cuando se pueda** (enmascarado). Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y **subidos** a la rama. No abras un PR si no te lo pido.
- Al final, dime lo que me toca comprobar en mi app **y para qué**, y deja al día los dos prompts (éste y el de la nube) con lo que siga.
