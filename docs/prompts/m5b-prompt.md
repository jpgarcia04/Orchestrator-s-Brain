# Prompt para M5b — replanteamiento visual y de alcance

Copia todo lo de abajo en una conversación nueva de Claude Code, con working directory en este repo.

---

Estoy construyendo **Orchestrator's Brain**, un motor de conocimiento personal local-first. La arquitectura y las decisiones ya están cerradas y documentadas — léelas antes de escribir nada:

1. [`docs/PROJECT-MASTER.md`](../PROJECT-MASTER.md) — documento vivo. Lee **§0** (tesis y métrica), **§1** (decisiones fundacionales, en particular **F3**), y el **informe de M5a completo**, incluida su sección «El pase visual» y la corrección «el mapa pasa a ser la pantalla».
2. `docs/adr/0012`, `0013` y `0014` — por qué el núcleo expone una superficie propia (`obrain-api`), por qué el mapa dejó de ser un fondo y pasó a ser la pantalla, y cómo conviven la galaxia y el explorador. `0013` **sustituye** a `0004`; lee los dos, en ese orden. `0014` es la decisión más reciente y **es la que gobierna este hito**.
3. [`docs/EXPERIENCE.md`](../EXPERIENCE.md) — el lenguaje visual. **§1** (la luz es información, no decoración) es la ley que no se negocia. **§4.7** (el conmutador galaxia/explorador), **§4.4** (el panel de entidad, con su alcance ya acotado) y **§4.1** (la escalera de navegación) importan especialmente para este hito.

**Código ya existente y funcionando** (léelo, no lo reescribas sin motivo):

- `core/obrain-core` a `core/obrain-embed` — M0 a M4. Log de aserciones, escaneo, enlazado, extracción, puerta de sensibilidad, embeddings. Estable, con tests.
- [`core/obrain-api`](../../core/obrain-api) — M5a. La superficie: `Brain::search`, `Brain::lens`, `Brain::sky`, `Brain::entity`, `Brain::correct`. `sky.rs` tiene la agregación por regiones (`regions`, `cluster`, puentes entre regiones). 215 tests en verde, clippy limpio.
- [`ui/`](../../ui) — Tauri v2 + TypeScript + PixiJS. `src/mapa.ts` es el renderizador (cielo de regiones + lente de vecindario, temperatura por recencia, polvo, latido). `src/main.ts` es el flujo actual: abres, ves el cielo, escribes, el mapa se enciende. `src/nucleo.ts` son los tipos del puente con Tauri.

Todo compila. `cargo test --manifest-path core/Cargo.toml` en verde, `cargo clippy -- -D warnings` limpio, `npx tsc --noEmit` limpio en `ui/`.

## Qué pasó en M5a, sin adornarlo

Se construyó la superficie del núcleo (buscar, agrupar, recorrer el grafo, ver una entidad), un flujo de aplicación (mapa de regiones al abrir, la barra enciende resultados agrupados por carpeta, Enter abre una lente sobre un nodo) y un pase visual sobre PixiJS (temperatura por recencia, densidad como polvo, latido, profundidad simulada).

Se probó **escaneando este mismo repositorio** — 101 archivos, casi todos código Rust y Markdown de este proyecto. Con eso se validó que la arquitectura no rompe: la lente acota, el layout es determinista, la agregación por regiones funciona, los 215 tests pasan.

El usuario vio la aplicación corriendo y el veredicto fue claro. En sus palabras:

> «no me gusta nada la UI... esto está muy lejano de una UI profesional»
>
> «entras y ves muy pocos nodos, nada navegable, la barra de búsqueda muy mal colocada (aún no es un chatbot)»
>
> «todos los nodos son de archivos de este proyecto... aún faltan muchísimas cosas que yo le tengo que pasar a mi computadora, principalmente de mi celular»
>
> «no hemos aclarado el tema de tener acceso o mínimo visibilidad a cosas en la nube en ciertas plataformas... con mi correo o algo así me conformaría»
>
> «si le picas [a un nodo] que te salga un tipo modal o algo así y podríamos crear como nuestro explorador de archivos... tal cual un explorador de archivos»
>
> «va bien gran mayoría de la lógica y el programa en sí, hay que meterle muchísimo a lo visual»

Es decir: **la arquitectura y la lógica se dan por buenas.** Lo que falla es la superficie — visual, de navegación, y dos preguntas de alcance sin cerrar.

> **Matizado el 2026-08-10, tras ejecutar el pipeline contra un disco real.** La arquitectura aguantó sin una grieta, pero «sólo falla la superficie» era un supuesto y resultó ser medio falso: aparecieron los problemas **#5** y **#6**, y los dos son del núcleo. Lee el estado actualizado al principio de la lista de problemas.

## Los problemas, en el orden en que hay que atacarlos

> **Estado al 2026-08-10.** El **#1** está **resuelto** — hay corpus real, ver abajo. El **#3** se cerró con decisión escrita, [ADR-0014](../adr/0014-dos-vistas-un-solo-estado.md), y lo que queda es construirlo. El **#4** lo aplazó el usuario. Queda abierto el **#2** (el pase visual, con prototipos ya construidos — ver abajo) y aparecieron **tres problemas nuevos: #5, #6 y #7**. Ninguno es de la capa visual. Léelos antes de tocar píxeles: invalidan el supuesto de que aquí sólo había que arreglar lo que se ve.

### 1. El corpus de prueba no es representativo — RESUELTO (2026-08-10)

**Ya hay corpus real y está construido.** Base en `%LOCALAPPDATA%\obrain\vida.obrain`: **1.042 archivos del usuario**, 4.026 vectores, 2.394 aristas, 276 concentradores, 44 duplicados, 28 entidades `sensitive` y 14 `vault`. La base de M5a (`brain.obrain`, los 101 archivos del repo) sigue intacta al lado; **no la uses para juzgar nada visual**.

Se llegó ahí tapando un agujero de exclusiones que se llevaba el 86 % del índice, y descartando un checkout de `whisper.cpp` y las partidas guardadas. El detalle completo, con las mediciones, está en el «Informe del primer corpus real (2026-08-10)» de `PROJECT-MASTER.md` §9. **Léelo: contiene los problemas #5 y #6 de esta lista.**

Buena noticia que conviene tener presente mientras diseñas: la consulta «comprobante de ingresos», cuyas palabras no están en el corpus, recupera los documentos de la visa del usuario en 21,8 ms. La tesis del §0 funciona sobre papeles reales. Lo que falla es cómo se ve y cómo se explica.

<details>
<summary>El planteamiento original, cuando esto estaba abierto</summary>

Con 101 archivos de código, 12 regiones es matemáticamente correcto (`sky::effective` usa `√archivos` con techo en el objetivo) pero se *ve* pobre y no dice nada de la vida de nadie. No pulas el layout ni el conteo de regiones contra este corpus — es una muestra de humo, no el producto.

Lo que hace falta:
- **El usuario ya contestó (2026-08-10): quiere amplitud.** *«desde mis descargas hasta mi papelera de reciclaje»*. Traducido a lo que hoy hace el código:
  - `Descargas`, `Documentos`, `Escritorio`, `Imágenes` y demás carpetas de perfil son escaneables **hoy**, sin tocar nada. Nada en [`core/obrain-scan/src/exclude.rs`](../../core/obrain-scan/src/exclude.rs) las poda. Empieza por ahí y mide el corpus real antes de dibujar nada.
  - La **papelera de reciclaje no**: `$Recycle.Bin` está en `SYSTEM_DIRS` de ese mismo módulo, y además cae por el atributo oculto/sistema. Que no se indexe no es un fallo: es una decisión de producto que nadie ha tomado todavía. Está anotada en §10 del maestro. **No la desactives por tu cuenta** — plantéasela al usuario con lo que implica (contenido que él decidió borrar reapareciendo en las respuestas, y la tensión con I4).
- El contenido del móvil (fotos, documentos, algún vídeo) todavía no ha llegado a la máquina. **Sigue sin llegar al 2026-08-10**, y es la masa que falta: 1.042 archivos son diez veces más que 101, pero no son todavía una vida entera. Hasta que llegue, cualquier decisión visual que dependa de la *densidad* real del grafo es provisional. No hay que fingir que hoy hay más de lo que hay — `EXPERIENCE.md` §5 (arranque en frío) ya describe cómo se supone que se siente un corpus pequeño y creciendo; vale la pena revisarlo y aplicarlo mientras el corpus sigue siendo chico, en vez de tratar el estado actual como el veredicto final sobre cómo se ve la app.

</details>

### 2. El pase visual necesita una decisión de herramienta tomada con los ojos abiertos, no heredada

Ya se intentó una vez quedarse en Pixi con un pase de shaders propio (halos, polvo, temperatura). El usuario lo vio y no le pareció profesional. **No repitas la misma apuesta por inercia.** Evalúa en serio las alternativas — three.js con post-procesado, WebGL2 a mano, Sigma.js, deck.gl, Cosmos, lo que sea — sin más restricción que la que ya está medida y escrita en el informe de M5a: el render no es el cuello de botella (0,1–1 ms de fotograma con Pixi), el layout si lo es (O(n²), 197 ms a 300 nodos en el hilo principal). Cualquier herramienta que elijas hereda ese presupuesto, no la implementación concreta.

**Antes de integrar nada en la aplicación real**, construye dos o tres prototipos visuales pequeños y aislados — capturas o una página suelta, no la app entera — y enséñaselos al usuario para que elija dirección. Ya se le enseñó una vez un resultado terminado y no le convenció; iterar dentro del ciclo completo de Tauri es lento y cuesta caro en tiempo de los dos. Prototipa barato primero, la app cara después.

> **Hecho el 2026-08-10 → [`prototypes/02-direcciones`](../../prototypes/02-direcciones/index.html).** Tres direcciones (Observatorio, Cartografía, Instrumento) sobre las regiones reales del corpus, con encendido por consulta, zoom, arrastre, entrada por clic con despliegue de subregiones y migas de pan. **La elección del usuario sigue pendiente.**
>
> Dos conclusiones ya firmes:
> - **La pregunta de la herramienta estaba mal planteada.** 20.800 nodos a 141 fps **en Canvas 2D**. No hizo falta three.js, deck.gl, Sigma ni WebGL. Lo que hacía falta era dejar de repintar lo que no cambia: el cielo en reposo se pinta una vez a un lienzo aparte, y mientras la cámara se mueve se **transforma** esa capa en vez de repintarla. Si el layout O(n²) aprieta —que es el cuello real medido— se saca del hilo principal; no se cambia de librería.
> - **El tamaño del nodo no puede escalar con el zoom.** Se probó y acercarse convertía el cielo en una mancha. Amortiguado (`z^0.34`), acercarse revela estructura, y de paso el tamaño sigue significando densidad de conexiones y no distancia de cámara, que es lo que exige §1.

### 3. El patrón de interacción — YA DECIDIDO (2026-08-10), no lo reabras

**Esta pregunta estaba abierta y se cerró. Lee [`docs/adr/0014`](../adr/0014-dos-vistas-un-solo-estado.md) y `EXPERIENCE.md` §4.7 antes de tocar la interfaz.** Lo que sigue es el resumen; el ADR es la fuente.

El problema de fondo era real y estructural, no estético: hoy no existe ninguna forma de **navegar sin buscar**. Ni la lente (vecindario alrededor de un foco que sólo se consigue escribiendo) ni el cielo (que se abre o se enciende, sin pila de navegación ni migas de pan) permiten entrar en una región, ver qué contiene, bajar un nivel y volver.

La resolución no fue elegir entre modal y panel lateral, sino distinguir **modal por consecuencia** (tocas un nodo y algo tapa el mapa — lo que §4.4 prohíbe, y sigue prohibido) de **modo por invocación** (pides otra vista con un gesto explícito). Lo que hay que construir:

- **La galaxia es el estado base.** ADR-0013 intacto.
- **Un conmutador** —visible en un sitio fijo **y** con atajo de teclado— abre el **explorador**: un panel central grande, tipo gestor de archivos, con migas de pan, columnas y navegación por teclado. **Nunca aparece solo**; siempre lo pide el usuario.
- **Al entrar, la galaxia se congela**: un fotograma, desenfocado, bucle de render parado. Una fotografía tras un cristal, no una escena viva.
- **Continuidad de estado en los dos sentidos, y es requisito, no acabado.** El conmutador cambia la lente, no el destino: el explorador abre en el nodo enfocado con su ruta en las migas; al volver, la cámara queda donde el explorador te dejó.
- **El explorador navega el grafo, no el disco.** Columnas de cualquier relación de contención, no sólo carpetas. Si sólo enseña carpetas, has construido el explorador de Windows con pasos de más y el proyecto pierde su tesis.
- **Fuera de la primera versión:** arrastrar y redimensionar libremente. Centrado, grande, tamaños preestablecidos. El contenido primero.

No hace falta ninguna pieza nueva en el núcleo: `sky::regions`, `sky::cluster`, `Brain::entity` y `Brain::lens` ya dan las dos vistas. Ver la tabla de correspondencia en el ADR.

### 4. Visibilidad de nube — APLAZADO por el usuario (2026-08-10)

El usuario lo dejó explícitamente para más adelante: *«lo de la nube pues todo a su tiempo, vamos viendo»*. **No abras esta conversación en M5b y no implementes nada en esa dirección.** F3 sigue vigente sin cambios. Lo de abajo se conserva porque describe cómo hay que plantearlo el día que se retome, no como tarea de este hito.

<details>
<summary>Planteamiento para cuando se retome</summary>

F3 en `PROJECT-MASTER.md` §1 dice, sin ambigüedad: *«Cuña v1: Encuentra lo que no recuerdas que tienes → Sin conectores de terceros en v1»*, y la tabla de esa sección advierte que cambiar una decisión F invalida el resto de lo construido sobre ella. El usuario ha dicho que con visibilidad de correo (o algo mínimo así) se conformaría, pero no hay una decisión tomada — es una idea suelta, no un alcance.

No implementes ningún conector. Antes de tocar código en esa dirección:
- Pregúntale qué necesita de verdad: ¿leer metadatos de correos para enlazarlos con archivos ya existentes (coherente con F1, local-first, si el procesamiento sigue siendo local)? ¿O acceso más amplio a servicios en la nube (rompe F3 de raíz)?
- Si la respuesta implica tocar F3, dilo explícitamente como lo que es — una revisión de una decisión fundacional, no un feature — y documenta la consecuencia en un ADR nuevo, no como una nota al margen.

</details>

### 5. El dimensionado de regiones — RESUELTO (2026-08-10), ver [ADR-0015](../adr/0015-el-cielo-reparte-por-anchura-no-por-peso.md)

**Se midió con `obrain-api --pesos` y la conclusión fue que había dos problemas, no uno.** El del *reparto* era un fallo real y está arreglado: `OneDrive` pasó de un grumo sin abrir de 851 archivos a cuatro regiones, y la primera región del cielo es ahora `OneDrive/Documentos` —donde vive el trámite de visa— en vez de un directorio de fuentes de terceros. El del *tamaño* no era un fallo: ninguna señal medible codifica importancia, y fabricar una puntuación sería lo que §1 prohíbe. El tamaño sigue siendo el número de archivos, a sabiendas, hasta M6.

**No lo reabras sin leer el ADR.** Y si vas a tocar el dimensionado, `--pesos` está en el núcleo para que midas antes de opinar.

<details>
<summary>El planteamiento original, cuando esto estaba abierto</summary>

Con el corpus real, la región mayor del cielo son **229 capturas de pantalla**. Las resoluciones del IMSS y los estados de cuenta del banco —o sea, la pregunta de la visa— viven en regiones de 12 y de 2 archivos.

Antes de descartar `whisper.cpp` era todavía más claro: **21 de las 30 regiones eran subcarpetas de un único repositorio de terceros.** [`sky::regions`](../../core/obrain-api/src/sky.rs) parte repetidamente la región mayor por peso de subárbol, y un árbol de código es profundo, ancho y uniforme, así que gana ese concurso una y otra vez. Mientras tanto `OneDrive` —donde está la vida— se quedaba como un solo grumo sin abrir.

**Por qué esto no es un umbral que retocar.** `EXPERIENCE.md` §1 ya legisló este canal visual para los nodos: *«Tamaño de nodo = densidad de conexiones. Nunca "importancia" arbitraria: eso sería una opinión disfrazada de dato»*. Las regiones usan otra regla para el mismo canal. Es una incoherencia dentro del lenguaje visual, y hay que resolverla con criterio —no subiendo un número hasta que el cielo se vea bonito con este corpus concreto, que sería repetir el error del que este documento venía advirtiendo.

Nota que la ley de §1 no te da la respuesta hecha: «densidad de conexiones» aplicada a una región tampoco es obviamente lo correcto. Piénsalo y **argúmentalo en un ADR**, porque cambia cómo se lee la primera pantalla del producto.

</details>

### 6. El boilerplate de los PDF contamina los vectores y la procedencia — NUEVO, y es del núcleo

Los cinco primeros resultados de «comprobante de ingresos» puntuaron 0.870, 0.870, 0.870, 0.870 y 0.869. Empatados a la milésima: el sistema acierta la *clase* de documento y dentro de la clase no discrimina.

El motivo está en el fragmento que enseña como prueba: un **sello digital del SAT en base64**, y para dos resultados distintos el mismo texto legal de relleno sobre la inflación, palabra por palabra. El relleno común a todos los estados de cuenta se trocea y se vectoriza como si fuera contenido, y al ser el texto más «documental» del archivo, gana la similitud.

**Esto te va a explotar en la cara en M5b y por eso está aquí y no en M6.** I9 dice que no hay respuesta sin procedencia, y §4.4 exige que el «¿por qué?» resalte el fragmento exacto del origen. El panel de entidad y la animación de Trazo —*la animación que vende el producto*, según §3— se apoyan las dos en enseñar ese fragmento. Si lo que aparece es un sello en base64, el gesto que más confianza tiene que generar es justo el que va a parecer roto.

No lo arregles a ciegas ni lo silencies filtrando por longitud o por entropía sin medir. Es el mismo patrón que M2 y M3 ya vivieron con las cachés de Chromium: los contadores decían éxito y **auditar el contenido** decía otra cosa. Audita primero.

### 7. El panel de entidad tiene que enseñar el archivo, y eso toca el criterio de M1

`EXPERIENCE.md` §4.1 lleva **previsualización** como escalón propio de la escalera desde el principio, y §4.4 describe el panel sin decir nunca qué se ve del archivo en sí. El usuario lo planteó sin rodeos el 2026-08-10: *«si no puedo ver los archivos, no va a tener mucho chiste la app»*. Tiene razón, y sin esto el explorador de [ADR-0014](../adr/0014-dos-vistas-un-solo-estado.md) es una lista de nombres.

Media infraestructura existe: `EntityView` ya lleva `path`, y el texto extraído de M2 está guardado como proyección. Lo que no existe es nada que lo enseñe.

**La trampa, y es de las que se pagan caras.** Un archivo `cloud_dehydrated` es un puntero: **tocarlo lo descarga**. M1 se definió entero por no hidratar ni uno, y una miniatura generada al vuelo —o peor, una previsualización al pasar el ratón sobre un nodo— lo violaría en silencio y a escala, que es como se pierde la confianza de golpe en un producto que promete no tocar tu nube.

La regla se deriva sola y no es negociable: **previsualiza sólo lo ya extraído y lo local; hidratar nunca como efecto secundario, sólo como acto explícito y dicho con todas las letras.** El `Availability` que M1 ya calcula es la señal; úsala, no la recalcules.

Las tres cosas son decisiones distintas y están sin tomar (ver §10 del maestro): previsualizar dentro, abrir en la app del sistema, revelar en el explorador. La segunda merece un aviso: obrain sigue siendo solo-lectura, pero el programa que abra el archivo no tiene por qué serlo.

## Qué NO hacer

- No pulir el layout, los umbrales de agregación o los colores contra el corpus de 101 archivos de este repo. Es una muestra de humo.
- No asumir que quedarse en PixiJS es la respuesta porque ya hay código escrito ahí. Evalúalo desde cero contra lo que el usuario realmente quiere ver.
- No dejar que el explorador aparezca **solo** (al tocar un nodo, al terminar una búsqueda, al arrancar). En cuanto aparece sin que lo pidan, es el modal que §4.4 prohíbe y ADR-0014 deja de sostenerlo.
- No entregar el explorador sin continuidad de estado en las dos direcciones. Es la parte que parece pulido y es en realidad el requisito: sin ella son dos aplicaciones compartiendo ventana.
- No construir el explorador contra el árbol de carpetas del disco. Se navega el grafo (`sky::regions`), no el sistema de archivos.
- No tocar ningún conector externo (correo, nube) sin que el usuario haya dicho explícitamente que eso implica revisar F3. **Aplazado por decisión suya el 2026-08-10** — no es un pendiente que haya que empujar.
- No subir el número de regiones ni tocar `sky::effective` hasta que el cielo «se vea bien» con este corpus. El problema #5 es de criterio, no de umbral, y ajustar el número contra mil archivos concretos es exactamente el error del que este documento venía advirtiendo.
- No tapar el problema #6 filtrando fragmentos por longitud o entropía sin haber auditado antes qué se está troceando. Los contadores van a decir que mejoró.
- No perder lo que sí funciona: la superficie del núcleo (`obrain-api`), la agregación por regiones, la separación hecho/inferencia, el presupuesto de I2.

> **Y una corrección al propio documento.** Arriba se afirma que *«el problema está en la capa visual y de interacción, no en la arquitectura de debajo»*. Con el corpus real eso resultó ser sólo medio cierto: los problemas #5 y #6 son del núcleo. La arquitectura sigue siendo buena —el log, la identidad, la bitemporalidad y la superficie aguantaron el primer disco de verdad sin una grieta— pero «sólo hay que arreglar lo que se ve» era un supuesto, y el disco lo desmintió.
