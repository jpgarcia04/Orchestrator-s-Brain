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

## Los cuatro problemas, en el orden en que hay que atacarlos

> **Estado al 2026-08-10.** De los cuatro, dos siguen abiertos y son el trabajo de M5b: **#1** (corpus) y **#2** (el pase visual). El **#3** se cerró — hay decisión escrita, [ADR-0014](../adr/0014-dos-vistas-un-solo-estado.md), y lo que queda es construirlo. El **#4** lo aplazó el usuario. Empieza por #1: sin corpus real, #2 se juzga contra humo otra vez.

### 1. El corpus de prueba no es representativo, y eso hay que decirlo antes de diseñar nada

Con 101 archivos de código, 12 regiones es matemáticamente correcto (`sky::effective` usa `√archivos` con techo en el objetivo) pero se *ve* pobre y no dice nada de la vida de nadie. No pulas el layout ni el conteo de regiones contra este corpus — es una muestra de humo, no el producto.

Lo que hace falta:
- **El usuario ya contestó (2026-08-10): quiere amplitud.** *«desde mis descargas hasta mi papelera de reciclaje»*. Traducido a lo que hoy hace el código:
  - `Descargas`, `Documentos`, `Escritorio`, `Imágenes` y demás carpetas de perfil son escaneables **hoy**, sin tocar nada. Nada en [`core/obrain-scan/src/exclude.rs`](../../core/obrain-scan/src/exclude.rs) las poda. Empieza por ahí y mide el corpus real antes de dibujar nada.
  - La **papelera de reciclaje no**: `$Recycle.Bin` está en `SYSTEM_DIRS` de ese mismo módulo, y además cae por el atributo oculto/sistema. Que no se indexe no es un fallo: es una decisión de producto que nadie ha tomado todavía. Está anotada en §10 del maestro. **No la desactives por tu cuenta** — plantéasela al usuario con lo que implica (contenido que él decidió borrar reapareciendo en las respuestas, y la tensión con I4).
- El contenido del móvil (fotos, documentos, algún vídeo) todavía no ha llegado a la máquina. Hasta que llegue, cualquier decisión visual que dependa de la *densidad* real del grafo es provisional. No hay que fingir que hoy hay más de lo que hay — `EXPERIENCE.md` §5 (arranque en frío) ya describe cómo se supone que se siente un corpus pequeño y creciendo; vale la pena revisarlo y aplicarlo mientras el corpus sigue siendo chico, en vez de tratar el estado actual como el veredicto final sobre cómo se ve la app.

### 2. El pase visual necesita una decisión de herramienta tomada con los ojos abiertos, no heredada

Ya se intentó una vez quedarse en Pixi con un pase de shaders propio (halos, polvo, temperatura). El usuario lo vio y no le pareció profesional. **No repitas la misma apuesta por inercia.** Evalúa en serio las alternativas — three.js con post-procesado, WebGL2 a mano, Sigma.js, deck.gl, Cosmos, lo que sea — sin más restricción que la que ya está medida y escrita en el informe de M5a: el render no es el cuello de botella (0,1–1 ms de fotograma con Pixi), el layout si lo es (O(n²), 197 ms a 300 nodos en el hilo principal). Cualquier herramienta que elijas hereda ese presupuesto, no la implementación concreta.

**Antes de integrar nada en la aplicación real**, construye dos o tres prototipos visuales pequeños y aislados — capturas o una página suelta, no la app entera — y enséñaselos al usuario para que elija dirección. Ya se le enseñó una vez un resultado terminado y no le convenció; iterar dentro del ciclo completo de Tauri es lento y cuesta caro en tiempo de los dos. Prototipa barato primero, la app cara después.

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

## Qué NO hacer

- No pulir el layout, los umbrales de agregación o los colores contra el corpus de 101 archivos de este repo. Es una muestra de humo.
- No asumir que quedarse en PixiJS es la respuesta porque ya hay código escrito ahí. Evalúalo desde cero contra lo que el usuario realmente quiere ver.
- No dejar que el explorador aparezca **solo** (al tocar un nodo, al terminar una búsqueda, al arrancar). En cuanto aparece sin que lo pidan, es el modal que §4.4 prohíbe y ADR-0014 deja de sostenerlo.
- No entregar el explorador sin continuidad de estado en las dos direcciones. Es la parte que parece pulido y es en realidad el requisito: sin ella son dos aplicaciones compartiendo ventana.
- No construir el explorador contra el árbol de carpetas del disco. Se navega el grafo (`sky::regions`), no el sistema de archivos.
- No tocar ningún conector externo (correo, nube) sin que el usuario haya dicho explícitamente que eso implica revisar F3. **Aplazado por decisión suya el 2026-08-10** — no es un pendiente que haya que empujar.
- No perder lo que sí funciona: la superficie del núcleo (`obrain-api`), la agregación por regiones, la separación hecho/inferencia, el presupuesto de I2. El problema está en la capa visual y de interacción, no en la arquitectura de debajo.
