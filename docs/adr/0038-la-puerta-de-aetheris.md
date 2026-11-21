# ADR-0038 — La puerta de Aetheris: localizar y leer, sin salir de lo normal

- **Estado:** Aceptada por JP el 2026-11-21. No adelanta nada del plan: se construye cuando toque (ver «Dónde va en el plan de confianza»).
- **Fecha:** 2026-11-21
- **Responde a:** el pedido de Aetheris «La puerta de Brain para Aetheris»
  (`Aetheris/docs/contratos/brain-localiza-y-lee.md`, rama `pedir-puerta-brain`, 2026-11-21).
- **Depende de:** [ADR-0006](0006-donde-vive-la-sensibilidad.md),
  [ADR-0012](0012-el-nucleo-expone-una-superficie.md),
  [ADR-0021](0021-cifrado-en-reposo-y-pin.md),
  [ADR-0034](0034-correcciones-y-revocacion-del-pin.md),
  [ADR-0036](0036-identidad-entre-bibliotecas.md),
  [ADR-0037](0037-actualizaciones-firmadas-desde-la-laptop.md)
- **Fuente:** `main` en `e520611`, leído sin ejecutar nada y sin abrir ninguna base.

## Contexto

Aetheris es la interfaz conversacional que JP quiere poner por encima de sus herramientas. Hoy
sólo lee DevHQ. Para usar Brain como segundo especialista pide una puerta MCP por stdio, apagada
de fábrica y de sólo lectura, con tres herramientas:

- `localizar`: nombres y rutas de lo normal ya clasificado. Es lo único que Aetheris enseñaría a
  un proveedor remoto como Claude.
- `leer`: pasajes con su procedencia, sólo para un modelo que corre en la laptop.
- `documento`: la ficha de un documento con sus afirmaciones, también sólo en local.

Y nueve requisitos (B1–B9) y cuatro tensiones. Brain es dueño de su repositorio: decide si la
construye, cómo y cuándo. Este ADR es su respuesta. No escribe código de la puerta.

## Lo que Brain comprobó en su código

Antes de contestar, cada cosa que el pedido da por hecha se miró en `main`:

| Pregunta | Respuesta |
|---|---|
| ¿Está la cuarentena de publicación en `main`? | **Sí.** `publication.rs` entró con `6d17965` (2026-10-12) y es antecesor de `main`. Las tres lecturas de contenido la respetan: BM25 (`obrain-extract/src/search.rs`), kNN (`obrain-embed/src/index.rs`) y la puerta de capacidades (`gate::check`). |
| ¿Se puede saber por documento que está clasificado en su versión actual (B5)? | **Sí, sin cambiar el esquema.** `content_publication.state = 'ready'` quiere decir que la última versión observada se extrajo y se clasificó: al ver una versión nueva vuelve a `pending` (`prepare_content`), una extracción fallida se queda en `pending` y sólo `publish_content` la pasa a `ready`. Un documento sin fila (carpetas, sesiones) cuenta como no clasificado. |
| ¿Puede `search` dar nombres y rutas sin `snippet` ni pasajes (B4)? | **No tal cual.** `Hit` lleva juntos `label`/`path` y `snippet`/`passages`. Y hay algo más importante: la búsqueda léxica **sólo busca en el texto** de los documentos (`chunks_fts`), no en sus nombres. Una lista de nombres sacada de `search` es información derivada del contenido. Ver «Ajusta» §1. |
| ¿Filtra `search` lo protegido? | **No del todo, y a propósito para la ventana.** Lo de bóveda sale en la lista con su nombre. Lo sensible sale con su fragmento **si no hay PIN puesto**, porque `contenido_visible()` es «no hay PIN, o está desbloqueado». La ventana se lo enseña a JP; una puerta no puede heredar esa regla. |
| ¿`entity` sirve para `documento`? | **Casi.** Ya quita evidencia y procedencia de lo sensible bloqueado, pero sólo cuando hay PIN. Además deja el nombre de otros documentos relacionados aunque sean sensibles, y trae los identificadores de aserción y `correctable`, que son el canal de corrección. |
| ¿Qué pasa con dos procesos abriendo la base cifrada? | **Leer a la vez, sí; abrir con `Brain::open`, no.** La base va en WAL con espera de 10 s, y la ventana y `obrain indexar` ya conviven así. Pero `Brain::open` no es una apertura de lectura: repara transiciones de respaldo a medias, reanuda o empieza el cifrado de una base en claro, **crea una base y su llave** si no existe, migra el esquema, recupera contenido heredado (clasifica y publica) y copia la base entera a una carpeta temporal para verificarla. Y mientras otro proceso la tenga abierta, restaurar un respaldo o mudar la base se niega con «base en uso». |
| ¿El PIN de la ventana alcanza a otro proceso? | **No.** El desbloqueo vive en la memoria de cada `Brain` (`desbloqueo: Cell<Option<Instant>>`). Un proceso nuevo nunca está desbloqueado. |
| ¿Hay ya una apertura sólo de lectura? | **No para el producto.** Sólo las verificaciones de respaldo abren copias en `SQLITE_OPEN_READ_ONLY`. |
| ¿Qué dice la política de Brain sobre salir? | `gate::decide`: lo normal puede ir a un modelo remoto **con consentimiento**; lo sensible, sólo a modelos locales; lo de bóveda, a ninguno fuera de la máquina. |

## Decisión

**Brain acepta construir la puerta, con ajustes, y no ahora.** La regla que ordena todos los
ajustes es una:

> **Lo que cruza la puerta ha salido de Brain.** Brain no puede comprobar dónde corre el modelo
> de Aetheris, ni qué guarda en sus registros, ni a quién se lo pasa después. Para el núcleo, la
> puerta es un destino remoto con consentimiento, aunque Aetheris lo use en local.

Es la tabla de `gate::decide` aplicada al proceso de al lado: lo normal sale si JP lo ha
consentido, y lo sensible y lo de bóveda no salen, ni enteros ni en forma de pista.

### Lo que Brain acepta

- **B1, una puerta MCP por stdio**, sin puerto ni red, cliente de la superficie de ADR-0012 como
  la ventana y la CLI. Sólo `initialize`, `ping`, `tools/list` y `tools/call`.
- **B2, apagada de fábrica.** Se enciende sólo desde la ventana de Brain. El ajuste se guarda
  dentro de la base cifrada, no en un archivo suelto, y la puerta lo lee antes de responder nada.
  Aetheris no tiene forma de cambiarlo desde la puerta.
- **B3, sólo lectura.** `correct`, las colecciones, las marcas de carpeta, la papelera, los
  respaldos y el PIN no existen en la puerta. Además se abre la base de modo que una escritura
  sea imposible, no sólo ausente (ver «Qué hay que construir»).
- **B4, `localizar` separado de `leer`.** Y en el núcleo, no en un filtro de la puerta: el tipo
  que devuelve `localizar` **no tiene ningún campo donde meter texto del documento**. Es el mismo
  truco que el `Signal` de ADR-0006, que no tiene dónde guardar la clave que encontró: el error no
  se puede cometer por descuido.
- **B5, sólo lo clasificado en su versión actual**, con `content_publication.state = 'ready'`,
  identidad confirmada (ADR-0036) y fuera de la papelera. Lo demás no sale.
- **B7, cada pasaje con su documento, su método y su fecha.**
- **B9, estados `no_disponible` con motivo** en vez de errores en texto.
- **La lente se queda fuera.** Brain está de acuerdo: cada herramienta más es una superficie más
  que probar contra inyecciones, y ADR-0012 ya pedía defender el número de operaciones.
- **El PIN nunca pasa por la puerta.** Ni se pide ni se recibe.

### Lo que Brain ajusta, y por qué

**1. `localizar` sólo habla de lo normal, y lo protegido no se cuenta.** El pedido quiere, aparte
de la lista, «cuántos documentos protegidos coincidieron». Como la búsqueda encuentra por el
texto, ese número dice algo del contenido de lo protegido: preguntar «VIH» y recibir «1
protegido» ya es una fuga, y un remoto puede hacer cien preguntas seguidas. En su lugar, cada
respuesta lleva un aviso **fijo**, el mismo para cualquier consulta: que la puerta no consulta lo
sensible, lo de bóveda ni lo que está sin clasificar. Aetheris puede decirle a JP «puede haber
más en lo protegido; míralo en Brain» sin que la frase dependa de la pregunta.

Sobre lo normal, Brain acepta el precio y lo escribe: que un documento normal salga en
`localizar` cuando se pregunta por algo dice que su texto tiene que ver con eso. Sin buscar en el
texto, la puerta no encontraría lo que JP no recuerda, que es la razón de ser de Brain. Lo normal
puede salir con consentimiento (`gate::decide`), y el texto del ajuste lo tiene que decir así.

**2. `leer` y `documento` tampoco dicen «existe, protegido».** Por la misma razón: un «hay un
documento sensible que coincide» sale del texto del documento sensible. En la ventana ese aviso
llega a los ojos de JP, dentro del proceso de Brain; por la puerta llega a un programa cuyo
registro y cuyo modelo Brain no controla. `leer` devuelve lo normal con su aviso fijo, igual que
`localizar`. `documento` con un identificador sensible, de bóveda, sin clasificar, en la papelera
o inexistente responde **lo mismo en los cinco casos** (`no_disponible`, motivo `fuera_de_la_puerta`),
para que la respuesta no confirme qué es. En la ficha de un documento normal, los documentos
relacionados que estén protegidos desaparecen; no se deja su nombre.

**3. La regla de la puerta no depende del PIN.** Hoy, sin PIN puesto, el núcleo enseña lo sensible
completo, porque es JP quien mira. La puerta no usa `contenido_visible()`: aplica siempre «sólo
lo normal», haya PIN o no, esté la ventana desbloqueada o no.

**4. Rutas relativas a su biblioteca en `localizar`.** La ruta completa lleva el nombre de usuario
de Windows y cómo está organizado el disco, y el remoto no la necesita para nada. `localizar` da
el nombre, la biblioteca y la ruta dentro de ella. La ruta completa, que sirve para abrir el
archivo, va sólo en `documento`, que es local.

**5. Dos niveles de consentimiento en el ajuste.** Si `leer` y `documento` dan texto de lo normal,
y `localizar` sólo nombres, encender la puerta no es una sola decisión. El ajuste tiene tres
posiciones: **apagada** · **sólo localizar** · **localizar y leer lo normal**. Con «sólo
localizar», `leer` y `documento` responden `no_disponible`, motivo `apagada`.

**6. La versión de `serverInfo` es la del contrato, no la de la aplicación (B8).** Con ADR-0037
Brain se actualiza a menudo; si `serverInfo.version` fuera la de la aplicación, Aetheris dejaría
de usar la puerta en cada actualización. La versión es la del contrato de la puerta (`1`, `2`…) y
sólo cambia cuando cambia la forma de una herramienta. La de la aplicación viaja aparte, como
dato informativo.

**7. Un `no_disponible` más.** Además de `apagada`, `base_cerrada` y `ocupada`:
`version_distinta`, cuando la base necesita una migración que sólo hace la ventana, o es de una
versión más nueva que la puerta. La puerta nunca migra ni repara; se para y lo dice. Y `ocupada`
cubre también una restauración, un cifrado o una mudanza a medias, que la puerta detecta sin
tocar nada.

**8. «Fecha» en B7 quiere decir dos fechas.** Cuándo leyó Brain esa versión del archivo (la de la
extracción publicada) y la fecha de modificación que observó en el disco. Un pasaje no tiene otra
fecha honrada que dar. El método de cada pasaje (`bm25` o `knn`) hay que añadirlo: hoy la rama va
en el resultado (`ranks`), no en cada pasaje.

**9. La puerta no tiene la base abierta mientras no trabaja.** Si la tuviera, la restauración de
respaldos y la mudanza de la base dirían «base en uso» mientras Aetheris esté encendido. Abre
para cada llamada y cierra al terminar, o tras un rato corto sin llamadas si abrir resulta caro
(hay que medirlo: abrir supone desenvolver la llave con Windows).

### Lo que Brain rechaza

- **Contar lo protegido** en `localizar` y **«existe, protegido»** en `leer`. Ver ajustes 1 y 2.
- **Que lo sensible salga con el PIN abierto en la ventana.** El pedido ya sigue la 0003 de
  Aetheris en esto; Brain lo cierra por su lado: el desbloqueo es de la ventana y dura cinco
  minutos desde que JP teclea (ADR-0034); no se presta a otro proceso. Cambiarlo no sería un
  ajuste del ajuste, sino un ADR nuevo con su coste escrito.
- **Usar `Brain::open` en la puerta.** Abrir así escribe, migra, cifra, repara y puede crear una
  base nueva. Una puerta de lectura no puede tener ese poder, ni por accidente.
- **Hablar con el proceso de la ventana por un canal local**, la alternativa que prevé la 0010 de
  Aetheris. No hace falta: leer a la vez ya funciona con WAL, y un canal hacia la ventana sería
  una segunda superficie dentro del proceso que tiene el PIN desbloqueado.
- **Que la puerta se descargue o se instale aparte.** Viaja en el mismo instalador firmado que la
  ventana (ADR-0037), para que su versión, la de la ventana y la de la base sean siempre la misma.

## Dónde va en el plan de confianza

El plan dice «no fusionar DevHQ/PassGuardian». Una puerta de sólo lectura no fusiona nada: Brain
no sabe nada de Aetheris ni lo necesita, y la puerta no da a Aetheris más que lo que la ventana ya
le enseñaría a JP de lo normal. Pero sí es una superficie nueva, y el plan pone la utilidad
cotidiana al final. Brain decide este orden:

1. **No se construye antes del punto 3 del plan** («filtrar elegibilidad antes de cortar»). Sin
   él, `localizar` daría respuestas vacías y falsas: si los diez primeros resultados de una
   consulta son sensibles y la puerta los quita después de cortar, el undécimo, normal, nunca
   sale. El filtro «sólo normal y publicado» tiene que ir dentro de la consulta, antes del
   `LIMIT`, igual que el de colecciones (ADR-0031). Es el mismo trabajo que el punto 3 ya pedía.
2. **Ni antes de que Aetheris haya medido su modelo local**, porque sin él Aetheris no puede usar
   la puerta con el remoto (lo dice su propio pedido) y una puerta sin cliente es superficie sin
   uso.
3. **Entra en el punto 6, «utilidad cotidiana»**, junto a «preguntas y comparaciones citadas», que
   es lo que Aetheris haría con ella. No adelanta a los puntos 4 y 5.
4. **Se valida sólo con bibliotecas sintéticas** y una base aislada (`OBRAIN_DB`), y con la
   batería de ataques de Aetheris pasada contra esa misma base desechable, nunca contra la bóveda
   real.

Dos piezas pequeñas se pueden adelantar si otra tarea las necesita antes, porque sirven por sí
solas: la apertura sólo de lectura y el filtro de elegibilidad dentro de la consulta.

## Qué hay que construir

En el núcleo (`obrain-api`):

- **Una apertura de lectura**, `Brain::abrir_para_leer` o parecida: la llave con
  `obrain_keys::unlock`, la base con `SQLITE_OPEN_READ_ONLY` y además `PRAGMA query_only = ON`.
  Sin crear, cifrar, migrar, reparar ni recuperar contenido heredado. Si la base pide cualquiera
  de esas cosas, se niega con el motivo que toque (`base_cerrada`, `ocupada`, `version_distinta`).
  No hace la copia temporal de verificación: esa copia protege las aperturas que escriben.
- **Un filtro de elegibilidad dentro de la búsqueda**: `sensitivity = 'normal'`, publicación
  `ready`, identidad confirmada, ruta vigente. En BM25 y en kNN, antes de cortar.
- **`localizar`**, un tipo nuevo sin campos de contenido: identificador, nombre, biblioteca, ruta
  relativa, carpeta, fecha de modificación, tamaño, colecciones, la rama y el puesto por los que
  salió, y el aviso fijo. `semantic` igual que hoy.
- **`leer`**, la búsqueda con el filtro y con el método y las dos fechas en cada pasaje.
- **`documento`**, la ficha de `entity` sin identificadores de aserción ni `correctable`, sin
  relacionados protegidos, y con la respuesta única para todo lo que no es normal y publicado.
- **El ajuste de tres posiciones**, guardado en la base y escrito sólo desde la ventana.

En la aplicación:

- **El interruptor en Ajustes**, con un texto que diga sin rodeos qué ve Aetheris en cada posición
  y que los nombres de carpetas y colecciones también salen.
- **El ejecutable de la puerta** dentro del instalador firmado. Puede ser el mismo ejecutable con
  un modo propio o uno pequeño a su lado; se decide al construirlo. En el modo puerta no se abre
  ninguna ventana ni se carga la interfaz.

Pruebas, con datos sintéticos:

- Los tipos de `localizar` no se pueden serializar con texto del documento (una prueba que recorre
  los campos).
- Una consulta cuyos diez primeros resultados son sensibles devuelve los normales que vienen
  detrás.
- Lo sensible, lo de bóveda, lo pendiente y lo de la papelera no aparecen ni cuentan, con PIN y
  sin PIN, con la ventana desbloqueada y sin ella.
- La misma respuesta de `documento` para un identificador sensible y para uno inventado.
- La puerta abierta no escribe nada en la base (se compara el archivo antes y después) y no
  impide restaurar un respaldo cuando no está atendiendo una llamada.
- Con la puerta apagada, o en «sólo localizar», las herramientas responden su `no_disponible`.

## Alternativas consideradas

**A. Que Aetheris filtre lo que la puerta le manda.** Es lo que el pedido evita, y Brain también:
un filtro en el cliente es una segunda definición de lo que puede salir (ADR-0012), y basta con
que una versión nueva de `Hit` añada un campo para que salga sin que nadie lo note.

**B. Una herramienta `search` única con un parámetro «remoto sí/no».** Rechazada: deja la
separación en manos de quien llama, que es justo quien no debe decidirla.

**C. `localizar` sólo por nombre y ruta, sin mirar el texto.** Cerraría la pista sobre el contenido
de lo normal, pero dejaría a Aetheris sin lo que Brain hace mejor: encontrar lo que no recuerdas
cómo se llama. Lo normal puede salir con consentimiento; se prefiere decirlo en el interruptor a
quitar la utilidad. Queda como opción si JP prefiere el interruptor más estricto: sería una cuarta
posición, no otra puerta.

**D. Seguir la política de la ventana (lo sensible visible sin PIN).** Rechazada: esa regla supone
que quien mira es JP, y por la puerta no mira JP.

## Consecuencias

**A favor**

- Lo que puede salir lo decide el núcleo con una regla que no depende del PIN ni de la ventana, y
  se prueba sin abrir ninguna.
- La cuarentena de publicación, el filtro de bóveda y la identidad entre bibliotecas, que ya
  existen, se reutilizan tal cual. No cambia el esquema ni la criptografía.
- El filtro de elegibilidad antes de cortar mejora también la búsqueda de la ventana (punto 3).

**En contra**

- Aetheris pierde el recuento de lo protegido. Sabrá que puede haber algo más, nunca si lo hay.
- La lista de `localizar` sí dice algo del texto de lo normal. Es un precio aceptado, no un
  descuido, y tiene que estar escrito en el interruptor.
- Los nombres de carpetas y colecciones los escribe JP y salen tal cual: una carpeta que se llame
  como un diagnóstico es información. Si está marcada, lo de dentro es sensible y no sale; si no,
  su nombre viaja.
- Igual que dice ADR-0034, esto no aísla de otros procesos de la misma cuenta de Windows: quien
  pueda ejecutar como JP puede abrir la base como la abre la puerta. El interruptor es un
  consentimiento, no una cerradura.
- Una superficie más que mantener, con su versión de contrato y su batería de pruebas.

## Cuándo se revisa

- Cuando JP acepte o cambie esta propuesta.
- Cuando termine el punto 3 del plan, o cuando Aetheris haya medido su modelo local: lo que llegue
  último pone fecha a construirla.
- Si Aetheris pide otra herramienta (la lente, por ejemplo): cada una con su propio contrato.
