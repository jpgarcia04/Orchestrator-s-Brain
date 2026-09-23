# ADR-0023 — Vencimientos: una fecha inferida por documento, y la del usuario gana

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Decide:** los «vencimientos (*¿qué me caduca pronto?*)» de la Fase 3 del maestro (§9)
- **Depende de:** [ADR-0005](0005-donde-vive-el-texto-extraido.md), [ADR-0006](0006-donde-vive-la-sensibilidad.md), [ADR-0021](0021-cifrado-en-reposo-y-pin.md), [ADR-0022](0022-ocr-local-con-el-de-windows.md)

## Contexto

Lo que más se le olvida a una persona con documentos personales no es dónde
los guardó, sino **cuándo dejan de valer**: la TIE, el pasaporte, la póliza,
la tarjeta sanitaria. La fecha ya está escrita en el propio documento, y desde
la Fase 3.3 el sistema también la lee en las fotos y en los escaneos (ADR-0022):
«VALIDEZ 15 11 2026» en la foto de una TIE sólo existe si alguien la ha leído.

El usuario lo pidió así: *«detecta fechas de caducidad en el texto; guárdalas
como aserciones inferidas, con su “¿por qué?”; la evidencia es la dirección del
fragmento, nunca copies datos sensibles; tienen que poder corregirse: la palabra
del usuario siempre gana (I5); y una tarjeta “Caduca pronto” en el Inicio, sin
moverle al diseño»*. Los ejemplos de la bóveda de prueba: «hasta el 31 de agosto
de 2027», «válida hasta el 15/11/2026», «vence el 30 de septiembre de 2026»,
«VALIDEZ 15 11 2026», «CADUCA 12/2028».

Antes de escribir código se le enseñaron las decisiones con coste, y eligió
(2026-09-23):

| Pregunta | Eligió |
|---|---|
| ¿Con cuánta antelación avisa la tarjeta? | **90 días**, igual para todo |
| ¿Y lo que ya caducó? | **Se queda en la tarjeta hasta que lo corrijas** |
| ¿Varias fechas en un documento? | **Una**: la de la palabra más clara y, si empatan, la más lejana |
| ¿Cómo se corrige? | **«cambiar» en la vista previa**, junto a «¿por qué?» y «es correcto» |
| Con PIN y bloqueado, ¿qué enseña la tarjeta de algo sensible? | **El nombre y su carpeta, sin la fecha** |

La última la matizó él: pidió «sólo el nombre y, si el nombre no es claro, una
mini descripción». La descripción tendría que salir del texto del documento,
que es justo lo que el PIN tapa; en su lugar se enseña **la carpeta** donde
vive («IMG_2291.jpg · en 01 Identidad»), que ya se ve en el explorador y da
contexto cuando el nombre lo puso la cámara.

## Decisión

### 1 · Sólo cuenta una fecha con una palabra de caducidad delante

Un documento está lleno de fechas —nacimiento, expedición, firma, pago— y casi
ninguna es un vencimiento. El detector no busca fechas: busca **una palabra de
caducidad seguida de una fecha**, con como mucho unas pocas palabras de relleno
entre medias («el», «de», «:», «fecha»…).

| Regla | Palabras | Peso | Confianza |
|---|---|---|---|
| `caducidad` | caduca, caducidad, fecha de caducidad, expira, expiración, date of expiry, expiry, expires | 3 | 0,85 |
| `validez` | validez, válido/a hasta, valid until | 3 | 0,85 |
| `vencimiento` | vence, vencimiento | 3 | 0,75 |
| `vigencia` | vigencia, vigente hasta | 2 | 0,70 |
| `hasta` | «hasta» suelto, **sólo** si en los 100 caracteres de antes aparece vigencia, vigente, válido, cobertura, periodo, duración, contrato, póliza, seguro o «desde» | 1 | 0,55 |

«Vence» pesa menos que «caduca» porque también se usa para pagos. «Hasta»
suelto es la señal más débil y sólo cuenta con contexto: «abierto hasta las
14:00» o «plazo hasta el viernes» no son vencimientos.

Si tras la palabra viene un **intervalo** («vigencia: del 1 de septiembre de
2026 al 31 de agosto de 2027»), cuenta la segunda fecha.

### 2 · Formatos

- «31 de agosto de 2027», «15/11/2026», «15-11-26», «15.11.2026»,
  «15 11 2026» (el OCR de una TIE), «12 MAR 2031» (pasaportes, con los meses
  abreviados en español y en inglés) y `2027-08-31`.
- **Sólo mes y año** («CADUCA 12/2028», «agosto de 2027»): el **último día** de
  ese mes, como en las tarjetas.
- **Siempre día/mes**, nunca al estilo de Estados Unidos: la bóveda es de
  documentos de España y México.
- Años de dos cifras, 20xx. Años fuera de 1990–2100 o días imposibles (31/02)
  no son una fecha.

### 3 · Una aserción inferida por documento: `expires_on`

- Predicado nuevo **`expires_on`** («caduca el»), cardinalidad `one`, método
  por defecto `inferred`. El objeto es **la fecha en texto ISO**
  (`2026-11-15`): es una fecha de calendario, no un instante, y guardarla como
  milisegundos obligaría a elegir una zona horaria que la haría cambiar de día
  según quién la mire.
- **Una por documento.** Si hay varias candidatas, gana la de más peso y, en
  empate, la más lejana, que suele ser la vigencia del propio documento: en una
  póliza, «vigencia hasta el 31/08/2027» y no «carencia hasta el 31/12/2026».
  La misma fecha repetida en dos fragmentos (se solapan) cuenta una vez.
- `source = expiry@v1`, la confianza de la regla y, como **evidencia, la
  dirección y nunca el texto**:
  `regla=validez fragmento=2 bytes=1180..1204 otras=1`. Los bytes son del
  documento entero; `otras` cuenta las fechas candidatas que se descartaron. Es
  lo que permite que el «¿por qué?» resalte en la vista previa el tramo exacto
  (EXPERIENCE §4.4) sin que el log guarde una sola palabra del documento.
- Es una etapa más del pipeline, **después de la puerta** y antes de
  vectorizar, y es **idempotente**: si el documento dice lo mismo que la vez
  anterior, no escribe nada; si ya no dice nada o dice otra fecha, retracta lo
  que escribió antes (I4) y afirma lo nuevo.

### 4 · La palabra del usuario gana (I5)

- Corregir escribe una aserción **`user`** sobre `expires_on`: otra fecha, o
  **`nunca`** («no caduca»). «Es correcto» escribe la misma fecha como del
  usuario. El detector **sólo retracta lo que escribió él** (`expiry@…`, método
  `inferred`), así que ningún reindexado pisa una corrección, y la inferencia
  corregida se conserva (qué creía el sistema y en qué se equivocó).
- La resolución es la de siempre, `Store::resolve`: por precedencia de método,
  el usuario manda.
- La ficha de la vista previa enseña **sólo la fecha que manda**. La inferida
  que corregiste sigue en el log, pero enseñarla debajo de la tuya haría dudar
  de cuál vale (EXPERIENCE §4.5: la aserción pasa de pálida a sólida).

### 5 · La tarjeta «Caduca pronto»

- En el Inicio, **sólo si hay algo que decir**: entre el buscador y
  «Recientes». Sin nada que caduque, no hay tarjeta (EXPERIENCE §7: nada de
  avisos que reclamen atención), y **no hay notificaciones**: la tarjeta está
  donde miras, no te interrumpe.
- Entra lo que caduca en **90 días o menos** y **todo lo que ya caducó**, hasta
  que lo corrijas —otra fecha o «no caduca»—. En orden de fecha: lo caducado
  arriba, en rojo («caducó hace 12 días»); lo que viene, debajo («en 53 días»).
- Cada fila abre el documento en la vista previa, donde está la fecha, su
  «¿por qué?» —con qué palabra se leyó, cuántas otras había, y el tramo
  marcado en el texto— y «cambiar».
- Desde la terminal, `obrain vencimientos` lista lo mismo.
- **Con PIN y bloqueado**, lo sensible sale con su nombre y su carpeta y **sin
  la fecha**: el núcleo no la manda (como el texto, ADR-0021), y la fila dice
  «desbloquea para ver la fecha». Van al final, para que ni el orden la delate.
  En la vista previa, bloqueada, tampoco sale.

## Alternativas consideradas

### A. Todas las fechas candidatas, cada una corregible

No se pierde ninguna. Rechazada por el usuario: el mismo documento saldría dos
o tres veces en la tarjeta y cada falsa alarma pediría su propia corrección.

### B. La fecha más próxima

Nunca avisa tarde, pero convierte la carencia de una póliza en «caduca tu
póliza». Rechazada por el usuario.

### C. Guardar el vencimiento en el eje de validez (I6)

Una aserción «vigente» con `valid_to` en la fecha de caducidad. Es elegante y
es la pregunta equivocada: el eje de validez dice *cuándo es cierto un hecho*,
y que la TIE caduca el 15/11/2026 es cierto hoy y lo seguirá siendo después.
Además, corregirla exigiría reescribir intervalos en vez de afirmar un valor.

### D. Un aviso por tipo de documento (6 meses para pasaportes)

Muchos países piden 6 meses de vigencia al viajar. Rechazada por el usuario por
ahora: es una regla más que puede equivocarse (un documento que menciona un
pasaporte sin serlo) y el «¿por qué?» tendría que explicarla.

### E. Buscar cualquier fecha y clasificarla con un modelo

Más cobertura. Rechazada: el modelo local es de embeddings, no de extracción,
y un sistema que no sabe decir *por qué* eligió una fecha incumple I9.

## Consecuencias

**A favor**

- La TIE fotografiada avisa sola de que caduca en 53 días: el OCR la lee, la
  regla `validez` la encuentra y la tarjeta la enseña.
- Todo es auditable: cada fecha dice con qué palabra se leyó, en qué fragmento
  y cuántas otras había; y cada corrección queda en el log para siempre.
- No hay datos nuevos fuera de la base cifrada, ni texto del documento en el
  log.

**En contra**

- **Una fecha mal leída por el OCR es una fecha mal leída.** «VALIDEZ 15 11
  2026» con un 1 leído como 7 da el 15/11/2076. La confianza baja y el «¿por
  qué?» lleva al tramo exacto para comprobarlo, pero la única garantía es
  mirarlo.
- **Lo que no tiene palabra de caducidad no se detecta**, y todavía no se puede
  añadir una fecha a mano a un documento donde el detector no vio ninguna:
  «cambiar» sólo aparece donde hay una fecha. Es lo siguiente si hace falta.
- **Lo caducado no se va solo.** Es lo que se eligió: una TIE vieja sigue en la
  tarjeta hasta que se le diga «no caduca». Con una bóveda que guarde
  históricos, eso son varios clics la primera vez.
- «Hoy» se calcula en el núcleo en UTC; entre la medianoche de Madrid y la de
  UTC, «en N días» puede ir un día corrido. La ventana lo recalcula con la hora
  local para lo que enseña.
