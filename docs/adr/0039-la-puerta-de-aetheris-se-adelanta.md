# ADR-0039 — La puerta de Aetheris se construye ahora, como pieza aparte

- **Estado:** Propuesta, 2026-11-24. Se da por aceptada cuando JP fusione el PR que la trae.
- **Fecha:** 2026-11-24
- **Cambia:** [ADR-0038](0038-la-puerta-de-aetheris.md), sólo «Dónde va en el plan de confianza»
  y lo que de él depende en «Qué hay que construir». Las herramientas, los ajustes y los rechazos
  del 0038 siguen como están.
- **Toca:** [PLAN-CONFIANZA](../PLAN-CONFIANZA.md) (puntos 3 y 6),
  [ADR-0030](0030-un-documento-dentro-de-otro.md) y
  [ADR-0032](0032-las-copias-tambien-estan-dentro.md) (de dónde sale la sensibilidad de una copia).
- **Fuente:** `main` en `846fa23`, leído sin ejecutar nada y sin abrir ninguna base.

## Contexto

El ADR-0038 aceptó la puerta y la dejó para el punto 6 del plan, «utilidad cotidiana», con dos
condiciones antes de construirla. Las dos se cumplen ya:

| Condición del 0038 | Estado |
|---|---|
| Punto 3: filtrar lo elegible antes de cortar | **Hecho** en BM25 y kNN (`846fa23`, PR #24). «Lo que puede salir» está una vez, en SQL, en `obrain-core/src/elegibilidad.rs`, y el modo `SoloNormal` (normal, publicación `ready`, ruta vigente, identidad confirmada, sin depender del PIN) existe y está probado, sin llamante. |
| Que Aetheris haya medido su modelo local | **Hecho** el 2026-11-21 (decisión 0011 de Aetheris). Su conclusión cambia el porqué de la puerta: con DevHQ el modelo local ya no aprende casi nada, y la prueba que falta, T3, es justo leer documentos de Brain. Sin puerta real no se puede hacer. |

El 0038 pedía además que la puerta no adelantara los puntos 4 y 5. Dejarla en el punto 6 hoy
significa esperar a migraciones, mudanzas, respaldos, retención y dependencias, que no son
condiciones de su seguridad, mientras el único cliente que la necesita está parado esperándola.

Del punto 3 queda, según [PROGRESO](../PROGRESO-CONFIANZA.md), «reconciliar duplicados, versiones
y sesiones». Antes de adelantar nada se miró qué parte de eso toca a la puerta.

## Lo que Brain comprobó en su código

| Pregunta | Respuesta |
|---|---|
| ¿Se reconcilian los duplicados? | **Sí, desde `662bdee` (2026-10-13).** `obrain_link::duplicates` retracta lo que afirmó y ya no es cierto; `tests/duplicados.rs` cubre una copia que cambia y una que se borra y vuelve. Lo que el plan sigue llamando «duplicados» está hecho en su parte de `duplicate_of`. |
| ¿Y las versiones y las sesiones? | **No.** `obrain_link::versions` y `obrain_link::sessions` escriben `version_of` y `occurred_during` (método `Inferred`), pero no retractan una arista que deja de cumplirse: un archivo renombrado sigue siendo «versión de» y una sesión no se deshace. Sólo se ven en la ficha de un documento (`entity`), que es lo que daría `documento`. |
| ¿Una copia de algo protegido es protegida? | **No siempre.** La sensibilidad sigue a lo que va dentro de un paquete (`part_of`, ADR-0030 y ADR-0032), pero no a las copias: `Store::resolve_sensitivity` mira lo dicho de la entidad, sus carpetas marcadas y sus partes. Un pasaporte en `01 Identidad` (marcada) con una copia idéntica en `04 Trámites` (sin marcar), y sin nada en el texto que el clasificador reconozca, deja la copia en `normal`. Visto leyendo el código, sin reproducirlo todavía. |
| ¿La puerta tiene ya cómo rechazar una base más nueva? | **La regla existe.** `opening::validate` rechaza un `user_version` fuera de `1..=SCHEMA_VERSION`. La apertura de lectura que pide el 0038 la tiene que usar sin pasar por `Brain::open`. |
| ¿Hay dónde guardar el ajuste de la puerta sin cambiar el esquema? | **Sí.** El log de aserciones es la fuente de verdad (ADR-0001): el ajuste puede ser una aserción del usuario con su predicado registrado, con historia y sin tabla nueva. Se decide al construirlo. |

El tercer punto es nuevo y es el que importa. En la ventana pasa lo mismo, pero ahí mira JP. Por la
puerta, la copia saldría con el texto de algo que JP protegió a propósito, que es lo que el 0038
dice que no sale «ni entero ni en forma de pista».

## Decisión

**La puerta deja el punto 6 y se construye ahora, como pieza aparte.** No arrastra consigo nada
más de los puntos 4, 5 y 6: ni otra utilidad cotidiana ni otra tarea de persistencia. Lo que el
0038 aceptó, ajustó y rechazó sigue igual. Cambian el cuándo y tres condiciones que se añaden.

### Lo que tiene que estar antes de encenderla

1. **Ninguna copia de algo protegido cruza la puerta.** `SoloNormal` deja fuera todo documento
   cuyo texto indexado (`text_hash`) coincide con el de una entidad que no es `normal`, dentro de
   la misma consulta y antes del `LIMIT`, como el resto del filtro. Un texto vacío no cuenta como
   coincidencia, igual que un archivo vacío no es duplicado de nada. Por texto y no por
   `duplicate_of`, por la misma razón que dio el ADR-0032: un PDF guardado otra vez tiene otros
   bytes y el mismo contenido. Esto cambia sólo lo que cruza la puerta: si la ventana y el nivel
   que se guarda deberían también seguir a las copias es otra decisión, que no se toma aquí.
   *Prueba:* original en carpeta marcada y copia sin marcar, en los dos órdenes de antigüedad; la
   copia no aparece en `localizar`, `leer` ni `documento`, y `documento` con su identificador
   responde lo mismo que con uno inventado.
2. **`documento`, en la versión `1` del contrato, no trae relaciones inferidas.** Ni `version_of`
   ni `occurred_during` mientras no se reconcilien: una relación que dejó de ser cierta, servida
   como dato de Brain, es peor que no darla. Trae las deterministas (carpeta, `duplicate_of`,
   `part_of`), y de ellas sólo las que apuntan a algo que pasa `SoloNormal` o a una carpeta. Cuando
   versiones y sesiones se reconcilien, entrarán con su método y su confianza, y eso sube la
   versión del contrato (ajuste 6 del 0038).
   *Prueba:* un documento con `version_of` y `occurred_during` no los trae; uno con un relacionado
   sensible no lo nombra.
3. **La apertura de lectura es de la puerta, no del punto 4.** No espera a «migraciones
   secuenciadas» ni a «mudanzas consistentes»: se niega con `version_distinta` a cualquier
   esquema que no sea el actual (más viejo pide la ventana; más nuevo, otra puerta), con `ocupada`
   ante una restauración, un cifrado o una mudanza a medias, y no tiene la base abierta mientras
   no atiende (ajustes 7 y 9 del 0038). Lo que el punto 4 arregle después sólo puede hacer menos
   frecuentes esos `no_disponible`.

Siguen las condiciones del 0038 que no cambian: el ajuste de tres posiciones escrito sólo desde la
ventana; ningún uso de `contenido_visible()`; validación sólo con bibliotecas sintéticas y una base
aislada (`OBRAIN_DB`), y la batería de ataques de Aetheris pasada contra esa misma base desechable,
**nunca contra la bóveda real**.

### Lo que no hace falta esperar

- **Reconciliar versiones y sesiones.** Con la condición 2, no llegan a la puerta. Sigue en el
  punto 3 del plan.
- **El resto del punto 4 y el punto 5.** Ninguno decide qué sale por la puerta.
- **Otra utilidad del punto 6.** «Preguntas y comparaciones citadas» las haría Aetheris con la
  puerta; Brain no tiene que construirlas antes.

### Orden de construcción

Cada paso con sus pruebas, en el orden en que cada uno sirve al siguiente:

1. La apertura de lectura (`SQLITE_OPEN_READ_ONLY`, `query_only`, la llave con
   `obrain_keys::unlock`, `validate`, los `no_disponible`), con la prueba de que el archivo no
   cambia y de que no impide restaurar un respaldo.
2. La regla de las copias en `SoloNormal`.
3. En el núcleo: `localizar` (el tipo sin campos de texto), `leer` (método y dos fechas en cada
   pasaje) y `documento` (sin aserciones corregibles ni relaciones inferidas, con la respuesta
   única para lo que no es normal y publicado).
4. El ajuste de tres posiciones y el interruptor en Ajustes, con el texto que dice que lo normal
   puede salir y que los nombres de carpetas y colecciones también.
5. El modo puerta del ejecutable (MCP por stdio, sin ventana), dentro del mismo instalador firmado
   (ADR-0037).
6. La batería de Aetheris contra una base sintética desechable. Sólo con ella pasada se conecta.

## Alternativas consideradas

**A. Dejarla en el punto 6, como dice el 0038.** Sin coste de trabajo, pero T3 de Aetheris espera
a migraciones, retención y dependencias que no deciden nada de la puerta. Rechazada.

**B. Adelantarla sin condiciones nuevas.** Rechazada: la copia de algo protegido saldría por la
puerta, y `documento` serviría relaciones que pueden no ser ciertas.

**C. Terminar todo el punto 3 primero.** Cerraría versiones y sesiones antes, pero no la copia
protegida, que no está en el punto 3, y retrasa la puerta por algo que la condición 2 ya deja
fuera. Rechazada.

**D. Que la sensibilidad siga a las copias en todo Brain (el nivel guardado).** Cerraría el hueco
también en la ventana, pero es la alternativa B que el ADR-0032 rechazó y toca la proyección de
sensibilidad, los vectores (ADR-0035) y lo que la ventana enseña. Queda abierta como decisión
propia; esta puerta no la necesita.

## Consecuencias

**A favor**

- Aetheris puede hacer T3 con documentos sintéticos y la puerta real, en vez de con una simulada.
- `SoloNormal` gana la regla de las copias antes de tener su primer llamante.
- La primera versión del contrato es más pequeña: menos campos que probar contra inyecciones.

**En contra**

- La puerta llega antes que la persistencia del punto 4: habrá más `no_disponible` (`ocupada`,
  `version_distinta`) de los que habría después. Es el lado seguro.
- `documento` sale sin «versión de» ni sesión, que son útiles. Volverán con su método y su
  confianza, en otra versión del contrato.
- Fuera de la puerta, la copia de algo protegido sigue en `normal` hasta que se decida D.
- Una superficie más antes de lo previsto, con su versión de contrato y su batería.

## Cuándo se revisa

- Cuando JP acepte o cambie esta propuesta.
- Si al construir la regla de las copias resulta que no cabe en la consulta antes del `LIMIT`.
- Cuando se reconcilien versiones y sesiones: entran en `documento` y sube el contrato.
- Si se decide D, porque la regla de las copias de la puerta pasaría a ser la de todo Brain.
