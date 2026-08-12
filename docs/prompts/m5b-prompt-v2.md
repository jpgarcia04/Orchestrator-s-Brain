# Prompt para retomar M5b — estado al 2026-08-12

Copia todo lo de abajo en una conversación nueva de Claude Code, con working directory en este repo.

---

Estoy construyendo **Orchestrator's Brain**, un motor de conocimiento personal local-first. Hay una sesión de trabajo del 2026-08-10 que dejó mucho hecho y varias cosas a medio decidir — este prompt es el traspaso de esa sesión, no el arranque desde cero. **Lee primero [`docs/prompts/m5b-prompt.md`](m5b-prompt.md) entero**: es el documento que abrió M5b, y cada uno de sus 7 problemas tiene una nota de estado (resuelto / decidido / aplazado / nuevo) que ya recoge lo que pasó. Este prompt no repite ese contenido, lo continúa.

Antes de escribir nada, lee también:

1. [`docs/PROJECT-MASTER.md`](../PROJECT-MASTER.md) — en particular §9 («Informe del primer corpus real», 2026-08-10) y §10 (decisiones pendientes, que creció bastante en esta sesión).
2. `docs/adr/0013`, `0014` y `0015` — el mapa como pantalla, el conmutador galaxia/explorador, y el reparto del cielo. Los tres son de esta sesión o inmediatamente anteriores y todavía frescos.
3. [`docs/EXPERIENCE.md`](../EXPERIENCE.md) §4.7 (el conmutador) y §1 (la ley de la luz).

## Qué hay hecho, con evidencia y no de memoria

**Hay corpus real construido y funcionando.** Base en `%LOCALAPPDATA%\obrain\vida.obrain`: 1.042 archivos del usuario (no del repo), 4.026 vectores, 2.394 aristas, 276 concentradores. La consulta semántica «comprobante de ingresos» —palabras que no están en el corpus— recupera los documentos de la visa del usuario en 21,8 ms. La base de M5a (101 archivos de este repo) sigue intacta al lado; no sirve para juzgar nada de esto.

**El agujero de exclusiones que se llevaba el 86% del índice está tapado.** `core/obrain-scan/src/exclude.rs` ganó una señal —directorio con punto colgando de la raíz del perfil— en vez de una lista más larga de nombres de herramientas.

**El reparto del cielo está arreglado y medido**, no sólo intuido: `OneDrive` pasó de un grumo sin abrir de 851 archivos a 4 regiones, y la primera estrella del cielo es ahora `OneDrive/Documentos` en vez de un repositorio de terceros. `obrain-api --pesos` queda en el núcleo para volver a medir si alguien toca esto. Ver [ADR-0015](../adr/0015-el-cielo-reparte-por-anchura-no-por-peso.md).

**El patrón de interacción está decidido y escrito**, no abierto: galaxia como estado base, explorador siempre invocado por un conmutador (nunca por tocar un nodo), continuidad de estado en los dos sentidos como requisito. Ver [ADR-0014](../adr/0014-dos-vistas-un-solo-estado.md) y `EXPERIENCE.md` §4.7.

**Hay un prototipo visual tocable**, no sólo capturas: [`prototypes/02-direcciones/index.html`](../../prototypes/02-direcciones/index.html). Tres direcciones (Observatorio, Cartografía, Instrumento) sobre las regiones reales del corpus, con zoom hacia el cursor, arrastre, clic para entrar con despliegue de subregiones, migas de pan clicables, y encendido de la barra de búsqueda. 20.800 nodos sintéticos a 141 fps **en Canvas 2D puro** — la capa en reposo se pinta una vez y se transforma mientras la cámara se mueve, en vez de repintarse. Esto ya contestó la pregunta de qué herramienta usar (ninguna especial hacía falta) y una parte de cómo tiene que comportarse el zoom (el halo no puede crecer linealmente con el nivel o el cielo se convierte en una mancha).

## Lo que sigue abierto, en el orden en que probablemente hay que atacarlo

### A. La dirección visual — a medio decidir

El usuario está entre **Observatorio** e **Instrumento**, y descartó Cartografía sin decirlo explícitamente (elegir entre las otras dos ya implica que la apuesta de "territorios con frontera" no es la suya). Se decidió **no cerrar esto todavía**, con dos razones:

- Las dos finalistas comparten geometría y navegación; la diferencia es de tratamiento (luz, contención, adorno), más cercana a una piel que a una decisión estructural.
- Tres problemas abiertos (B, C y D de abajo) van a cambiar qué hay en pantalla, no cómo se pinta. Elegir paleta ahora es maquillar una composición que todavía va a cambiar de proporciones.

**El usuario mandó una imagen de referencia** — un fondo de pantalla astronauta/océano/galaxia del que salió la intención visual original del proyecto. Confirma que `EXPERIENCE.md` §2 («un observatorio de noche, o agua profundo») capturó bien la idea, pero la imagen es notablemente más saturada (magenta, cian) que los tres prototipos, y la luz en la imagen se acumula en **crestas continuas**, no en puntos discretos — una idea de render distinta a lo prototipado. **Guardada el 2026-08-12** en [`docs/referencias/astronauta-oceano-galaxia.png`](../referencias/astronauta-oceano-galaxia.png), con una nota de lectura en el [README](../referencias/README.md) de esa carpeta. Ahí está anotada la tensión que el pase de estilo tiene que resolver: la imagen sube la saturación, y §1 le tiene asignado un significado —la saturación codifica **confianza**—, así que subir el suelo estrecha el rango que queda para decir «esto es una inferencia dudosa». Si se sube, hay que reservar el margen antes, no después.

No cierres la dirección visual sin que el usuario la elija explícitamente. No es tu llamada.

### B. El boilerplate de PDF contamina los vectores y la procedencia — RESUELTO (2026-08-12), y el diagnóstico de abajo era el equivocado

> **Auditado y arreglado. Ver [ADR-0016](../adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md).**
> Auditar primero —que es lo que este apartado exigía— tumbó las dos mitades
> del diagnóstico:
>
> - **Los sellos son el 0,5 % del índice** (19 fragmentos de 4.058, en 8
>   archivos). No había volumen que contaminara nada.
> - **La recuperación acertaba por la razón correcta**: el fragmento ganador
>   dice «REPRESENTACIÓN IMPRESA DE UN CFDI», y CFDI es *Comprobante Fiscal
>   Digital*.
> - **El empate a 0.870 no era falta de discriminación: eran los mismos
>   bytes.** Dos de los cinco resultados son PDF recopilatorios que contienen a
>   los otros tres. 6 pares comprobados, 6 idénticos.
> - **Lo roto era el recorte**: la prueba eran los primeros 160 caracteres del
>   fragmento, sin buscar nunca la parte que había coincidido.
>
> Arreglado eligiendo la ventana ([`evidence.rs`](../../core/obrain-extract/src/evidence.rs)),
> no filtrando el índice: no se tiró un dato ni subió la versión del chunker.
> `obrain-extract --pruebas` mide si el índice puede explicarse.
>
> **Lo que dejó abierto** y está en §10 del maestro: cómo se representa que un
> documento esté *dentro* de otro. Son 467 fragmentos, el 11,5 % del índice, y
> es contenido legítimo duplicado — no se filtra, se modela.

<details>
<summary>El planteamiento original, cuando esto estaba abierto</summary>

La consulta «comprobante de ingresos» devuelve cinco resultados empatados a la milésima (0.870 × 4, 0.869 × 1) porque el fragmento que se vectoriza y se enseña como prueba es un sello digital del SAT en base64 y texto legal de relleno idéntico entre documentos. Esto rompe I9 (ninguna respuesta sin procedencia) y va a hacer quedar mal el panel de entidad y la animación de Trazo —la que "vende el producto"— en cuanto existan. **Antes de tocarlo, audita qué se está troceando** (es el mismo patrón que M2/M3 vivieron con las cachés de Chromium); no lo arregles filtrando por longitud o entropía sin medir primero.

</details>

### C. El panel de entidad necesita previsualización, y choca con el criterio de M1 — problema #7 del prompt original, sigue sin tocar

El usuario preguntó directamente: *"si no puedo ver los archivos, no va a tener mucho chiste la app"*. Tiene razón y `EXPERIENCE.md` §4.1 ya tenía "previsualización" como escalón de la escalera. La mitad de la infraestructura existe (`EntityView.path`, texto extraído por M2 ya guardado). **La trampa real**: 14 archivos del corpus son `cloud_dehydrated` — punteros de OneDrive. Previsualizarlos sin cuidado los descarga, violando en silencio el criterio que M1 se definió por cumplir. La regla que se derivó y que hay que respetar: previsualizar sólo lo ya extraído y lo local; hidratar nunca como efecto secundario, sólo como acto explícito del usuario. El `Availability` que M1 ya calcula es la señal — úsala, no la recalcules.

Son tres decisiones distintas sin tomar (detalle en `PROJECT-MASTER.md` §10): previsualizar dentro de la app, abrir con el programa del sistema, revelar en el explorador de Windows. La segunda merece un aviso en la interfaz: obrain sigue siendo solo-lectura, pero el programa que abra el archivo no tiene por qué serlo.

### D. Regiones creadas por el usuario — pedido explícitamente, sin diseñar

*"Quiero poder ordenar mis cosas a mi gusto y que el programa no pierda de vista qué es qué y con qué se relaciona."* La mitad difícil ya está garantizada por arquitectura: I5 dice que una aserción del usuario gana para siempre sobre cualquier inferencia, y `Brain::correct` ya la escribe con test. Lo que falta diseñar es estructural: hoy las regiones del cielo salen únicamente del árbol de carpetas del disco; esto introduce **una segunda fuente de regiones que no es el sistema de archivos**, conviviendo con la primera. Hay que decidir si una región de usuario es una entidad con una faceta propia, cómo se resuelve que un archivo caiga en dos regiones a la vez, y qué manda al dibujar el cielo cuando las dos fuentes compiten por el mismo archivo. Detalle completo en `PROJECT-MASTER.md` §10.

### E. M5c — vigilancia continua, ya tiene hito pero cero código

Añadido al roadmap en esta sesión con criterio de hecho verificable (`PROJECT-MASTER.md` §9): USN Change Journal + `ReadDirectoryChangesW`, va después de M5b. No es trabajo de esta sesión, pero no lo pierdas de vista si el usuario pregunta por qué las cosas nuevas no aparecen solas todavía — la respuesta es que es el hito siguiente, no que falte diseñarlo.

## Qué NO hacer

- No cierres la dirección visual (Observatorio vs. Instrumento) sin que el usuario elija. Tampoco insistas en cerrarla ya: se decidió esperar a B, C y D a propósito.
- No reabras el reparto del cielo (#5/ADR-0015) ni el patrón de interacción (#3/ADR-0014) — están decididos y medidos, no en discusión.
- No toques conectores de nube/correo (problema #4 del prompt original). Sigue aplazado por decisión del usuario, no por olvido.
- No previsualices ni generes miniaturas de archivos `cloud_dehydrated` como efecto secundario de nada. Ver sección C.
- No arregles el boilerplate de PDF (sección B) sin auditar primero qué se está troceando.
- Antes de tocar `sky::regions` o cualquier dimensionado, corre `obrain-api --pesos` sobre `vida.obrain`. La herramienta para medir ya existe; no opines sin ella.
