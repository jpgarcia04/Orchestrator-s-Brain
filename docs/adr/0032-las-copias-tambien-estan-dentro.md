# ADR-0032 — Las copias también están dentro

- **Estado:** Aceptada
- **Fecha:** 2026-09-24
- **Cambia:** [ADR-0030](0030-un-documento-dentro-de-otro.md) §2.4 («Textos idénticos, una vez») y su último «En contra»
- **Toca:** [ADR-0020](0020-sensibilidad-por-carpeta.md) (de dónde sale la sensibilidad)

## Contexto

ADR-0030 decidió que, de cada grupo de documentos con **el mismo texto**,
sólo contara uno —el más antiguo, el criterio de `duplicate_of`— al buscar lo
que está dentro de un paquete. Así el paquete no decía «contiene: B, B, B», y
una copia llegaba al paquete por su «Duplicado de».

Quedó apuntado como pendiente que la copia no decía «Está dentro de». Al
mirarlo (2026-09-24) salió algo más serio, y se comprobó con un experimento en
el banco de pruebas, con datos inventados:

| La bóveda | El paquete |
|---|---|
| El documento en `01 Identidad` (marcada) y una copia **más vieja** en `04 Trámites` | **Normal: sin tapar** |
| El mismo, pero la copia de `01 Identidad` es la más vieja | Sensible |

El paquete hereda la sensibilidad de lo que lleva dentro (ADR-0030 §4), pero
sólo veía al original. Si tu pasaporte de `01 Identidad` es la copia nueva y
hay otra vieja en otra carpeta, el PDF de la visa salía **sin tapar**: justo
lo que ADR-0030 §4 quería evitar. Que quede tapado o no dependía de qué copia
es más antigua. Lo mismo pasaba con una copia **del paquete** (el PDF de la
visa guardado dos veces): la copia no tenía ninguna relación y no heredaba
nada.

Se le enseñaron al usuario tres salidas con su coste: que cuenten todas las
copias, que sólo la copia diga «está dentro de» (sin cerrar el hueco), o
dejarlo. Eligió que **cuenten todas**.

## Decisión

1. **Se sigue comparando una vez por texto** —el más antiguo—, que es lo que
   hace la pasada barata. Pero **lo que se encuentra vale para todas las
   copias**: cada copia de B `part_of` cada copia de A, con la misma evidencia
   (el mismo tramo: el texto de las copias de A es el mismo). Cambia
   `obrain_link::dentro::link`; la fuente sigue siendo `linker:parrafos@v1`
   porque lo que se detecta no cambia, sólo a quién se le dice.
2. **La sensibilidad no cambia de regla.** Como cada copia tiene su
   `part_of`, `Store::resolve_sensitivity` ya las ve todas: el paquete es tan
   sensible como la más sensible de ellas, y se recalcula en el acto cuando
   marcas la carpeta de una copia, como con lo demás de dentro.
3. **«Contiene» no repite.** Las copias con el mismo texto (su `text_hash`)
   van en **una fila**, que encabeza la que sigue en tu bóveda y, de ésas, la
   más protegida —la que nombra el «¿por qué?» de la sensibilidad—: *«Movimientos
   marzo · y 1 copia»*. Su «¿por qué?» dice en qué carpetas están las copias.
   `obrain paquetes` dice lo mismo: *«contiene marzo.txt (6 de 6 párrafos, y 1
   copia con el mismo texto)»*.
4. **Cada copia dice «Está dentro de»**, y un clic abre el paquete por donde
   está, como el original.

## Alternativas consideradas

### A. Sólo que la copia lo diga, a través de su original

Barato: en la vista previa de la copia, leer el `part_of` de su original. No
cierra el hueco de sensibilidad, y sólo funciona entre copias idénticas al
byte (`duplicate_of`), no entre dos archivos distintos con el mismo texto.
Rechazada por el usuario.

### B. Seguir la cadena de `duplicate_of` al resolver la sensibilidad

Cerraría el hueco sin escribir más aristas. Rechazada: `duplicate_of` es por
hash del archivo y el paquete compara textos, así que un PDF guardado otra vez
(mismo texto, otros bytes) se quedaba fuera; y la vista previa seguiría sin
decirlo.

### C. Dejarlo

Rechazada: un documento de identidad dentro de un paquete tiene que tapar el
paquete, sea la copia que sea.

## Consecuencias

**A favor**

- El paquete se tapa si **cualquier** copia de lo que lleva está protegida,
  sin depender de cuál es más antigua.
- Cada copia sabe en qué paquete está, y la copia de un paquete también
  contiene lo que lleva.
- La pasada cuesta lo mismo: se compara una vez por texto.

**En contra**

- **Más aristas**: con `n` copias de lo de dentro y `m` del paquete, `n × m`
  en vez de una. En una bóveda personal son pocas; en el mapa, la lente de un
  paquete enseña también las copias.
- En tu base de agosto, `obrain paquetes` puede decir «y 1 copia» donde antes
  no decía nada, y el número de «dentro de otro» del informe de `indexar`
  sube: cuenta pares, copias incluidas. Los paquetes siguen siendo los mismos.
