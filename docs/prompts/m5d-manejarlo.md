# Prompt para M5d — «Manejarlo» · estado al 2026-08-13

Copia todo lo de abajo en una conversación nueva de Claude Code, con working directory en este repo.

---

Estoy construyendo **Orchestrator's Brain**, un motor de conocimiento personal
local-first. **M5b acaba de cerrarse y de usarse**, y este prompt sale de lo que
falló al usarlo — no de una lista de features. Lee antes de escribir nada:

1. [`docs/PROJECT-MASTER.md`](../PROJECT-MASTER.md) — §0 (la tesis), §2 (los
   invariantes, en particular **I2** y **I10**), y en §9 **«Por qué M5b se
   encogió»** y el **informe de M5b**. Los dos explican el método de este
   proyecto mejor que cualquier resumen.
2. [`docs/EXPERIENCE.md`](../EXPERIENCE.md) — **§1** (la luz es información: la
   ley que no se negocia), **§4.1** (la escalera de navegación) y **§4.7** (el
   conmutador).
3. [`docs/adr/0013`](../adr/0013-el-mapa-es-la-pantalla.md) y
   [`0014`](../adr/0014-dos-vistas-un-solo-estado.md) — el mapa es la pantalla;
   la galaxia y el explorador son dos vistas de un solo estado. **Este hito toca
   la superficie de los dos y no debe reabrir ninguno.**

## El hito, entero, en una frase

> **Puedes acercarte a donde quieras del mapa, y sabes qué vas a abrir antes de
> abrirlo.**

Dos piezas. Hay además tres arreglos pequeños al final que no son un hito y que
están acotados a propósito. **Si al terminar hay una sexta cosa, el hito creció**
— y este proyecto ya tuvo que corregir eso una vez, el 2026-08-12.

## De dónde sale este prompt

M5b cerró con el método escrito: *«cuando M5b cierre, se usa una semana antes de
escribir otra línea»*. El usuario lo abrió el mismo día y esto es lo que dijo,
literal:

> «mucho mejor el tema del explorador, hace falta pulirlo mucho»
>
> «no sé si me encanta que se vaya abriendo ventanas en horizontal mientras
> manejas por una carpeta, **sí me gusta lo de la ruta que se vaya viendo arriba
> dónde estás**»
>
> «tiene que ser una funcionalidad que puedas abrir, que sea un **botón visible
> en la galaxia** que sea el que te abre el explorador de archivos, no sólo con
> ctrl e»
>
> «la barra de búsqueda como que estorba un poco, quizás más esquinada o sólo un
> icono, estaría bien evaluarlo»
>
> «**sigue sin haber zoom, no te puedes mover y ampliar a tu gusto**, y está muy
> extraña la selección de nodos, sigue oxidado, ya está mejor que antes pero
> luego **no se entiende qué es lo que estás intentando abrir**»

Es el método funcionando: la lista sale de usarlo, no de imaginarlo.

## El hallazgo que ordena el hito, y ya está comprobado

**La cámara ya existe, está medida, y nunca se portó a la aplicación.**

| | Cámara (zoom/arrastre) | Manejadores de ratón |
|---|---|---|
| [`prototypes/02-direcciones/index.html`](../../prototypes/02-direcciones/index.html) | `cam {x, y, z}`, zoom interpolado en logaritmo, conversiones pantalla↔mundo, rejilla de fondo que acompaña, subregiones que entran con fundido por encima de zoom 2,2 | `wheel`, `pointerdown`, `pointermove`, `pointerup` — líneas 783–807 |
| [`ui/src/mapa.ts`](../../ui/src/mapa.ts) | **ninguna** | **cero** `wheel`, **cero** `pointerdown` |

`mapa.ts` sólo escucha `pointermove`, y para el paralaje. Todo se coloca una vez
para caber en la ventana (`recolocar`) y a partir de ahí el mapa **respira pero
no se deja mover**. Por eso «no te puedes mover y ampliar a tu gusto» no es una
sensación: es literal.

Y el prototipo trae dos conclusiones que §9 ya dio por firmes y que **no hay que
volver a decidir**:

- **20.800 nodos a 141 fps en Canvas 2D.** La pregunta de la librería estaba mal
  planteada; lo que hacía falta era dejar de repintar lo que no cambia.
- **El tamaño del nodo no puede escalar con el zoom.** Amortiguado (`z^0.34`),
  acercarse revela estructura y el tamaño sigue significando densidad de
  conexiones y no distancia de cámara, que es lo que exige §1. Está en el
  prototipo como `zVis`.

**Esto hace que la pieza 1 sea sobre todo un port**, no una invención. Es la
parte cara ya pagada.

## Pieza 1 — La cámara

Zoom con la rueda **hacia el cursor** (no hacia el centro: acercarse a una
estrella y que se te escape es lo que hace que un mapa se sienta ajeno),
arrastre con el ratón, y una forma de volver al encuadre completo sin adivinar.

Lo que más se rompe al implementarlo, y por qué:

- **El paralaje y la cámara se pisan.** `mundo.position` **ya lleva el paralaje
  dentro** — lo mete `latir` en cada fotograma— y eso ya causó un fallo real: en
  `regionEn` se restaba dos veces y se picaba la estrella de al lado. Con una
  cámara encima son tres transformaciones sobre el mismo contenedor. Decide
  **una** composición y que todo el hit-testing salga de ella; hoy hay dos
  funciones (`regionEn`, `nodoEn`) que repiten la conversión a mano.
- **I10 no se negocia:** *la animación jamás está en el camino crítico de la
  información*. El zoom puede interpolarse; lo que se ve y lo que se puede picar
  no puede esperar a que termine.
- **El layout es el cuello, no el render.** Está medido desde M5a: O(n²), 197 ms
  a 300 nodos en el hilo principal. La cámara no lo toca —mover la vista no
  recoloca nada— y **no lo arregles en este hito** salvo que te estorbe de
  verdad.
- **La estabilidad del layout es un requisito de §7.4**: misma consulta, mismas
  posiciones. La cámara cambia el encuadre, nunca las posiciones.

## Pieza 2 — Saber qué vas a abrir

Ésta es la mitad que el usuario describe como «muy extraña» y «oxidada», y tiene
una causa concreta en el código:

> [`mapa.ts`](../../ui/src/mapa.ts): *«Sólo el foco lleva etiqueta: trescientas
> serían trescientas razones para no mirar el mapa»* — `if (n.distance !== 0) continue`.

Es decir: **en una lente de hasta 300 nodos, 299 son puntos sin nombre.** La
decisión de no etiquetarlos todos es correcta y no hay que revertirla. Lo que
falta es todo lo demás:

- **No hay estado de «encima».** `nodoEn(x, y)` existe desde M5b y **sólo se
  llama al hacer clic**. No hay cambio de cursor, ni realce, ni el nombre del
  nodo bajo el puntero. Se pica a ciegas.
- **No hay estado de «seleccionado».** Después de abrir el panel, el mapa no
  dice cuál de los trescientos puntos es el que estás mirando.
- **Acercarse debería revelar nombres, como en el prototipo.** Ahí las
  subregiones entran con un fundido por encima de zoom 2,2. Es la respuesta
  natural a «trescientas etiquetas»: no se enseñan todas, se enseñan **las que
  caben a la escala a la que estás**.

Criterio práctico: **antes de pulsar tienes que poder decir en voz alta qué vas
a abrir.** Si hay que picar para averiguarlo, la pieza no está hecha.

## Y tres arreglos que NO son un hito

Acotados a propósito. Si alguno crece, sale del hito y se anota.

### 1. El conmutador no se ve — y eso es un fallo contra ADR-0014

El botón **existe**: `#conmutador`, abajo a la derecha, texto «explorador», con
`Ctrl+E` en su `title`. El usuario lo usó y aun así pidió «un botón visible en la
galaxia». Eso no es una petición de una función nueva: es el veredicto de que
**12 px en `--tinta-tenue` en una esquina de una pantalla negra no cumplen la
palabra «visible»** de la consecuencia #2 del ADR. Arreglar la presencia, no
añadir un segundo botón.

### 2. La barra estorba — y esto sí toca una decisión, así que pregúntalo

Hoy `#barra` son 560 px centrados a 24vh del alto: en medio del mapa. El usuario
propone «más esquinada o sólo un icono» y pide **evaluarlo**, no que lo decidas.

Lo que hay que tener delante al planteárselo:
[ADR-0013](../adr/0013-el-mapa-es-la-pantalla.md) decidió que *el mapa es la
pantalla y la barra busca dentro de él*, sustituyendo a
[ADR-0004](../adr/0004-punto-de-entrada-barra-no-galaxia.md), que ponía la barra
delante. Encoger la barra va **a favor** de 0013, no en contra — pero 0004
existía porque escribir es «el camino de las cuarenta veces al día», y un icono
que hay que pulsar antes de escribir añade un gesto a ese camino. La salida
probable es un atajo que la despliegue y que se repliegue sola, pero **no la
elijas tú**: enséñale dos o tres opciones y que escoja.

### 3. El explorador: qué le gustó y qué no

- **Le gustan las migas.** *«sí me gusta lo de la ruta que se vaya viendo arriba
  dónde estás»*. **No las toques.**
- **Le incomodan las columnas que crecen en horizontal.** Ojo aquí:
  [ADR-0014](../adr/0014-dos-vistas-un-solo-estado.md) decidió las columnas con
  su motivo escrito y rechazó explícitamente la alternativa C. **No lo reabras
  por una frase que empieza con «no sé si me encanta».** Lo que sí puede
  arreglarse sin tocar el ADR es el síntoma: que el ancestro ocupe lo mismo que
  el nivel activo, que haya que desplazarse en horizontal para ver dónde estás,
  que a cuatro niveles no quepa nada. Estrechar o colapsar los niveles de arriba
  —como hace Finder— conserva la decisión y quita la molestia. **Arréglalo y
  vuelve a preguntarle.** Si con eso sigue sin gustarle, entonces sí es el ADR y
  entonces se reabre con datos.
- «hace falta pulirlo mucho» es una frase sin bordes. **Pídele que la concrete
  contra la aplicación abierta** antes de tocar nada; media hora suya vale más
  que un día tuyo adivinando.

## La decisión que este hito desbloquea

**¿Observatorio o Instrumento?** (§10 del maestro). La dirección visual estaba
aplazada *«a propósito hasta después de M5b»* porque el explorador y el panel
iban a cambiar las proporciones de la pantalla. **Ya las cambiaron.** Y el
usuario ha dicho «sigue oxidado», que es exactamente el vocabulario de esa
decisión.

Los tres prototipos están en
[`prototypes/02-direcciones`](../../prototypes/02-direcciones/index.html) —
Cartografía quedó descartada de hecho. **Es del usuario, no tuya.** Enséñaselos
y que elija. Y ten presente la tensión que §10 ya dejó anotada: la referencia
visual sube la saturación, y §1 le tiene asignado un significado —codifica
**confianza**—, así que subir el suelo estrecha el rango disponible para decir
«esto es una inferencia dudosa». Si se sube, el margen se reserva antes.

## Qué NO hacer

- **No metas nada más en el hito.** Modo deriva, expansión, tecla global,
  Trazo, regiones creadas por el usuario y `Process` están fuera y por escrito
  (§9). Si crees que alguno hace falta para cerrar el criterio, es que el
  criterio se está reinterpretando.
- **No reabras** ADR-0013 (el mapa es la pantalla), ADR-0014 (las dos vistas y
  las columnas), ADR-0015 (el reparto del cielo) ni ADR-0016 (la prueba se
  elige). Están decididos y medidos.
- **No cambies de librería de render.** Ya se planteó y se contestó con un
  número: 20.800 nodos a 141 fps en Canvas 2D. El problema nunca fue Pixi.
- **No etiquetes los 300 nodos de la lente.** La solución es la escala, no el
  volumen.
- **No arregles el layout O(n²)** salvo que la cámara lo destape de verdad. Está
  medido y anotado; sacarlo a un worker es otro trabajo.
- **No previsualices ni generes miniaturas de `cloud_dehydrated` como efecto
  secundario de nada.** M5b lo dejó cerrado y con test; no lo aflojes al mover
  la interacción.
- **No toques conectores de nube ni de correo.** Aplazado por decisión del
  usuario; F3 sigue vigente.
- **No escribas un ADR por cada arreglo.** Un ADR es para una decisión que
  alguien reabriría dentro de seis meses. Si eliges la dirección visual o mueves
  la barra, eso **sí** es un ADR. Cambiar un color, no.

## Estado del que partes, comprobado y no de memoria

- `cargo test --manifest-path core/Cargo.toml` → **263 en verde**.
- `cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings` → limpio.
- `cargo check` en `ui/src-tauri` → limpio. `npx tsc --noEmit` en `ui/` → limpio.
- Corpus real en `%LOCALAPPDATA%\obrain\vida.obrain`: **1.130 archivos**, 30
  regiones. La app se arranca con
  `OBRAIN_DB="$LOCALAPPDATA/obrain/vida.obrain" npm run app` desde `ui/`.
  **Cierra la ventana anterior antes**: si hay una corriendo, cargo no puede
  sobrescribir el `.exe` y falla con «Acceso denegado».
- Herramientas de auditoría en el núcleo, de hitos anteriores:
  `obrain-extract --sample`, `obrain-extract --pruebas`, `obrain-api --pesos`,
  `obrain-api --cielo`, `obrain-api --explorar`, `obrain-api --ver <id>`.
  **Mide con ellas antes de opinar** — es la convención del proyecto y ya ha
  desmentido dos diagnósticos escritos.

### Un número que conviene tener presente

El 2026-08-12 el diagnóstico era: *«el núcleo son 15.264 líneas y la aplicación
que se puede abrir, 1.578. Este documento, 1.685 — el maestro es más largo que
el producto»*.

Hoy: núcleo **16.744**, aplicación **3.382**, maestro **1.828**. La aplicación
se ha duplicado y ya es casi el doble que el documento. **La interfaz está
alcanzando al motor**, que era el problema de fondo. Sigue por ahí.

## Sobre el orden frente a M5c

M5c (vigilancia continua) sigue pendiente y va **después** de esto. El motivo no
es preferencia: M5c hace que aparezcan cosas nuevas en el mapa, y un mapa que no
se deja mover ni dice qué vas a abrir es un mal sitio donde ver aparecer nada.
Además el coste de esperar es bajo — reescanear el perfil entero cuesta 0,60 s,
así que un temporizador cubre el hueco sin deuda arquitectónica. **Conviene
confirmarlo con el usuario en la primera respuesta**, no darlo por hecho.

## Y cuando cierre

**Vuelve a usarlo antes de escribir otra línea.** Funcionó: M5b se cerró, se
abrió la aplicación, y de ahí salió este prompt entero. Es la única prueba que
decide si esto es un producto o una demostración muy bien documentada.
