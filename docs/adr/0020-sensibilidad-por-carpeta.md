# ADR-0020 — Sensibilidad por carpeta: una marca del usuario que hereda lo de dentro

- **Estado:** Aceptada. Su primer «En contra» (renombrar la carpeta le quita la marca) lo resuelve [ADR-0027](0027-la-carpeta-sigue-a-su-fileid.md)
- **Fecha:** 2026-09-22
- **Acota:** [ADR-0006](0006-donde-vive-la-sensibilidad.md) (su alternativa D)
- **Depende de:** [ADR-0018](0018-bibliotecas.md)

## Contexto

La puerta de sensibilidad (`core/obrain-guard`, [ADR-0006]) decide el nivel de
cada archivo con dos clases de señales: **el nombre** y **el texto que M2 ya
extrajo**. En el banco de pruebas de la Fase 2, las dos fotos de la TIE
—`IMG_2291.jpg`, `IMG_2292.jpg`— salieron `normal`: una foto no tiene texto
hasta que haya OCR, y su nombre lo puso una cámara. La única señal de contexto
que existía, `rules::classify_parent`, mira el **nombre** de la carpeta madre, y
`01 Identidad` no contiene ninguno de sus términos.

El usuario lo pidió con las palabras exactas: *«Quiero poder marcar una carpeta
como sensible (por ejemplo `01 Identidad`) y que todo lo que tenga dentro lo
herede, también lo que llegue después. Tiene que ser una aserción del usuario
(I5: gana siempre y ningún reescaneo la pisa)»*.

[ADR-0006] ya había considerado la herencia por carpeta y la había **rechazado
como modelo** (alternativa D) por tres razones:

1. Un archivo secreto en `Descargas` se quedaba fuera.
2. Una carpeta `Contratos` teñía los veinte mil archivos que colgaran de ella.
3. Chocaba con I8: la ruta es una observación, no la identidad; colgar un
   control de seguridad de la ruta significa que **mover un archivo cambia sus
   permisos**.

Las dos primeras dejan de aplicar cuando la herencia **no sustituye** a la
clasificación por archivo sino que se suma a ella, y cuando no la infiere el
sistema sino que **la decide el usuario** sobre una carpeta que él mismo ordenó.
La tercera sigue siendo cierta, y esta decisión la acepta a sabiendas.

## Decisión

**El usuario puede marcar una carpeta, y todo lo que está debajo de ella hereda
como mínimo ese nivel.**

1. **La marca es una aserción del usuario sobre la carpeta**:
   `contents_sensitivity = sensitive | vault`, método `user`, cardinalidad `one`.
   El clasificador sólo retracta lo que escribió él, así que ningún reescaneo la
   pisa (I5). Una marca que no sea del usuario no cuenta, y una marca `normal`
   no es una marca.
2. **Es un suelo, no un nivel.** Sube lo de dentro y nunca lo baja: un `.env`
   de bóveda dentro de una carpeta sensible sigue siendo bóveda. Una carpeta no
   puede declarar «aquí nada es sensible».
3. **Cubre por ruta, a cualquier profundidad.** Lo de dentro es lo que **ahora**
   está debajo de la ruta de la carpeta —subcarpetas incluidas, y sin distinguir
   mayúsculas—, no lo que alcanzan las aristas `contained_by`. Así hereda lo que
   llegue después sin reescribir nada, y no depende de que M3 retracte la arista
   vieja de un archivo que se movió (hoy no lo hace).
4. **Gana la última palabra del usuario.** Si el usuario dijo algo del propio
   archivo, eso manda —también para bajarlo, como en [ADR-0006]— **salvo** que
   una carpeta que lo contiene se marcara *después* con un nivel mayor. Marcar
   `04 Trámites` sube el folleto que hace un mes dijiste que era normal; decir
   «normal» de ese folleto después de marcar la carpeta es una excepción
   consciente y se respeta.
5. **Vive en el único puente.** La regla está en
   `Store::resolve_sensitivity` (`obrain-core/src/sensitivity.rs`), que es lo
   que usa `reproject_sensitivity`. La columna `entities.sensitivity` sigue
   siendo el punto de control y la puerta sigue leyendo una sola columna: nada
   de lo que decide el egreso resuelve el log.
6. **Marcar recalcula en el acto** todo lo que hay debajo, en una transacción
   (`obrain_guard::mark_folder`). Cada `indexar` vuelve a aplicarla a lo nuevo:
   archivos y también **carpetas**, para que una subcarpeta nueva enseñe su
   candado.
7. **Lo que sube a bóveda sale del índice de significado** en la pasada
   siguiente de vectorización (`index::purge_withheld`), con su procedencia
   (`embedded_with`, `vector_count`) retractada. Antes había que reconstruir el
   índice a mano; con carpetas marcables como bóveda, ese hueco se volvía
   alcanzable con un clic.

En la interfaz, un botón en la cabecera de cada carpeta: **«Marcar como
sensible»** con un clic; **«Sensible»** cuando está marcada, y quitarla pide un
segundo clic porque baja la protección; **«Sensible por «X»»** cuando la marca
es de una carpeta de fuera, que lleva hasta ella. La vista previa dice *«por la
carpeta «01 Identidad», que marcaste tú»* y la vista «Sensibles» lista las
carpetas marcadas con cuántos documentos protegen. Desde la terminal,
`obrain sensible <carpeta> [--boveda | --quitar]` y `obrain sensibles`.

## Alternativas consideradas

### A. Escribir la marca en cada archivo (materializar)

Al marcar, una aserción `sensitivity_level` del usuario en cada archivo de la
carpeta. Sobrevive a mover el archivo fuera, que es su única ventaja.
Rechazada: **no cubre lo que llega después** —que era la mitad del pedido— sin
un segundo mecanismo que lo haga; llena el log con N aserciones del usuario que
el usuario no escribió, y quitar la marca obliga a retractar N cosas y a
distinguir las que puso la carpeta de las que puso él.

### B. Heredar por las aristas `contained_by`

Es el árbol que ya dibujan el explorador y el mapa. Rechazada porque M3 no
retracta la arista vieja cuando un archivo se mueve: un archivo movido cuelga a
la vez de su carpeta nueva y de la vieja, y heredaría de las dos. Y el árbol es
una proyección cuya forma cambia (las bibliotecas de [ADR-0018] ya lo cortan
por arriba); un control de seguridad no debería depender de la forma de una
vista.

### C. «Marcar la carpeta» como atajo para marcar lo que hay hoy

Un `mark_by_user` por archivo, en bloque. Rechazada por lo mismo que A: lo que
llegue mañana no estaría marcado, y el usuario lo pidió expresamente.

### D. Dejar que una carpeta también baje el nivel

Una marca `normal` que dijera «aquí nada es sensible». Rechazada: bajar el
nivel de todo un árbol de golpe es exactamente el tipo de decisión que expone
un `.env` sin que nadie lo vea. Las excepciones se hacen archivo por archivo, y
quedan en el log una a una.

## Consecuencias

**A favor**

- **La foto de la TIE es sensible** sin OCR, sin nombre útil y sin texto: basta
  con que esté en `01 Identidad`.
- **Lo que llega después ya nace protegido**, en el primer `indexar` que lo ve.
- **Es reversible y auditable.** Quitar la marca la retracta, no la borra (I4):
  *¿qué protegía yo en marzo?* es una consulta bitemporal sobre
  `contents_sensitivity`.
- **La puerta no cambia.** Sigue siendo una columna con tres valores; lo único
  nuevo es cómo se calcula.

**En contra**

- **La marca va por ruta, y renombrar o mover la carpeta la deja atrás.** Si
  `01 Identidad` pasa a llamarse `01 Identidad y viajes`, M3 la ve como una
  carpeta nueva, la marca se queda en la ruta vieja y lo de dentro **vuelve a su
  nivel sin avisar**. Es el coste de la alternativa D de [ADR-0006] que aquí se
  acepta, y es un fallo hacia el lado inseguro. Mitigación de hoy: la vista
  «Sensibles» enseña cada carpeta marcada con los documentos que protege, y una
  vacía sale en rojo —*«vacía — ¿la renombraste o la moviste?»*—. La solución de
  verdad es que una carpeta conserve su identidad al renombrarse (el `FileID` de
  un directorio en NTFS), y queda pendiente. **Resuelto en [ADR-0027]
  (2026-09-23):** la carpeta sigue a su `FileID`, y la marca con ella; sólo
  moverla a otro disco la deja atrás.
- **Mover un archivo fuera le quita la herencia.** Es deliberado —la marca es
  una regla sobre un sitio, como un cajón con llave—, y lo que el clasificador
  diga de él por su nombre o su texto sigue valiendo. Para que un archivo sea
  sensible vaya donde vaya, se marca **ese archivo**.
- **Marcar no cifra nada.** «Sensible» significa hoy: borroso hasta que lo
  pides, fuera de cualquier modelo remoto y sólo en el índice local. El cifrado
  en reposo es la pieza siguiente de la Fase 3 y es otra decisión.
- **Resolver cuesta una consulta más por entidad** (su ruta) cuando hay alguna
  marca. Con una bóveda personal —cientos o pocos miles de archivos— no se
  nota; sin marcas no se paga.

[ADR-0006]: 0006-donde-vive-la-sensibilidad.md
[ADR-0018]: 0018-bibliotecas.md
[ADR-0027]: 0027-la-carpeta-sigue-a-su-fileid.md
