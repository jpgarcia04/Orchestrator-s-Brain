# ADR-0014 — Dos vistas, un solo estado: la galaxia y el explorador

- **Estado:** Aceptada
- **Fecha:** 2026-08-10
- **Complementa a:** [ADR-0013](0013-el-mapa-es-la-pantalla.md)
- **Acota:** [`EXPERIENCE.md`](../EXPERIENCE.md) §4.4

## Contexto

[ADR-0013](0013-el-mapa-es-la-pantalla.md) decidió que el mapa fuera la pantalla y la barra buscara dentro de él. M5a lo implementó y el veredicto visual fue negativo, pero debajo de la queja estética había un fallo estructural que ninguna paleta arregla:

**Hoy no existe ninguna forma de navegar sin buscar.**

Las dos superficies que hay responden a preguntas distintas y ninguna responde a ésta:

- La **lente** (I2) responde *«esto y lo que tiene al lado»*. Necesita un foco previo, y el único modo de conseguir un foco hoy es escribir una consulta.
- El **cielo** (`sky::regions`) responde *«cómo es el paisaje»*. Se abre o se enciende, pero no tiene pila de navegación: no se puede entrar en una región, ver qué contiene, bajar un nivel y volver.

El usuario lo dijo con el caso concreto, que es la forma correcta de decirlo: *«nada de que no te dio la app para llegar a una ruta que estaba dentro de 50 nodos más»*.

Y pidió, por su nombre, un modal tipo explorador de archivos. Eso choca de frente con [`EXPERIENCE.md`](../EXPERIENCE.md) §4.4, que dice que el panel de una entidad **nunca** es un modal porque *«perder el contexto espacial es perder el producto»*.

**La tensión es aparente, y se disuelve distinguiendo dos cosas que se ven iguales en pantalla y son opuestas para quien las usa:**

| | Cómo aparece | Qué se siente |
|---|---|---|
| **Modal por consecuencia** | Tocas un nodo y una superposición te tapa el mapa | El sistema te quitó el contexto. Es lo que §4.4 prohíbe, y con razón. |
| **Modo por invocación** | Pides otra vista con un gesto explícito | Tú apartaste el contexto porque ahora estorba. Es una herramienta, no una interrupción. |

§4.4 legisla sobre la primera. Lo que se pidió es la segunda.

## Decisión

**Existen dos vistas sobre el mismo estado, y el usuario alterna entre ellas cuando quiere.**

1. **La galaxia es el estado base.** ADR-0013 no se toca: al abrir se ve el cielo, y la barra busca dentro de él.

2. **El explorador es un segundo modo, siempre invocado explícitamente** — un conmutador visible y un atajo de teclado, los dos. Nunca aparece como consecuencia de tocar un nodo. Ésa es la línea que separa esta decisión de lo que §4.4 prohíbe, y no es negociable: en cuanto el explorador aparezca solo, se convierte en el modal que §4.4 rechaza.

3. **Al entrar al explorador, la galaxia se congela.** Se pinta un fotograma, se difumina y el bucle de render se detiene. Detrás del explorador hay una fotografía, no una escena viva. Se ve igual y respeta el compromiso de modo reposo de §2 — una galaxia animada bajo una ventana durante media hora es batería gastada en algo que nadie mira.

4. **Continuidad de estado en los dos sentidos. Es requisito, no mejora.** El conmutador cambia **la lente, no el destino**:
   - Galaxia → explorador: abre en el nodo o región enfocada, con su ruta ya en las migas de pan.
   - Explorador → galaxia: la cámara queda sobre el nodo donde estabas.

   Si alguna de las dos direcciones te devuelve al inicio, dejan de ser dos vistas y pasan a ser dos aplicaciones compartiendo ventana. Cuando eso pasa, el usuario elige una y no vuelve a tocar la otra nunca.

5. **El explorador navega el grafo, no el disco.** Las columnas no son únicamente carpetas: son cualquier relación de contención del grafo. Se baja por carpetas, y también por *entidad → dónde aparece → documentos que la mencionan*. Se siente como el explorador del sistema; por debajo es otra cosa. Sin esto, es el explorador de Windows con pasos de más.

6. **§4.4 se acota, no se deroga.** En modo galaxia, seleccionar un nodo abre un panel lateral y el mapa sigue vivo — exactamente como está escrito. En modo explorador ya estás en otro contexto y no hay mapa que destruir.

No hace falta ninguna pieza nueva en el núcleo. Las dos vistas son dos proyecciones del mismo dato que [`obrain-api`](../../core/obrain-api) ya expone:

| Pieza existente | Qué es en la galaxia | Qué es en el explorador |
|---|---|---|
| `sky::regions` | Las regiones del cielo | El árbol de navegación |
| `sky::cluster` | Los resultados agrupados al encender | El contenido de la carpeta actual |
| `Brain::entity` | El panel lateral | El panel de detalle |
| `Brain::lens` | El vecindario dibujado | «Relacionados», desde la fila seleccionada |

## Alternativas consideradas

### A. Sólo el panel lateral, sin explorador

Es la lectura conservadora que planteaba el prompt de M5b: un panel que ocupa casi toda la pantalla pero deja el mapa en un borde. Respeta §4.4 sin tocar nada.

Rechazada porque **no resuelve el problema real**. Un panel lateral sigue necesitando un foco previo, y el foco hoy sólo se consigue escribiendo. El agujero era navegar *sin* buscar, y esta alternativa lo deja intacto: arregla dónde se dibuja el detalle, no cómo se llega hasta él.

### B. El explorador como pantalla o pestaña aparte

Rechazada por la consecuencia #4. Una pestaña no comparte posición con la galaxia, y en el momento en que no la comparte son dos productos. El coste de esta alternativa no se paga el día que se construye, se paga a los tres meses, cuando se descubre que nadie usa la galaxia.

### C. Migas de pan dentro de la galaxia, sin explorador

Rechazada por legibilidad, que es el mismo motivo por el que existe el presupuesto de I2. Una jerarquía profunda dibujada sobre un layout de fuerzas no se lee: a cinco niveles de anidamiento el grafo es una maraña, y el caso que el usuario nombró —una ruta cincuenta nodos adentro— es precisamente donde peor se comporta. La jerarquía profunda tiene una representación que lleva cuarenta años ganando, y es la lista con columnas.

## Consecuencias

**A favor**

- Cierra el agujero de navegar sin buscar, que era estructural y no estético.
- Da un camino **determinista y verificable** hasta cualquier nodo. Eso sirve al usuario, y de paso sirve para auditar el propio grafo: hoy, si un archivo no aparece en una búsqueda, no hay forma cómoda de comprobar si está indexado.
- No añade ninguna pieza al núcleo. Son dos lecturas del mismo árbol de contención que M3 escribió y ADR-0013 empezó a leer.
- Respeta la escalera de §4.1: el modo es una tecla, como todos los demás escalones.

**En contra**

- **Agrava el riesgo que ADR-0013 ya dejó escrito** — *«el cielo se quedará siendo un explorador de archivos bonito, que es exactamente lo que este proyecto no quiere ser»*. Si el explorador resulta ser lo cómodo y la galaxia lo bonito, el usuario vivirá en el explorador y el producto habrá sido un gestor de archivos con salvapantallas. La mitigación no es prohibir el explorador: es que enseñe lo que un explorador de archivos no puede enseñar —facetas, lo inferido con su «¿por qué?», los puentes entre regiones— y observar honestamente cuál de las dos vistas se usa. Si a los tres meses la galaxia no se abre, el problema es la galaxia y habrá que decirlo.
- **El estado compartido entre las dos vistas es donde van a estar los bugs.** Es el precio de la consecuencia #4 y se paga entero: qué está enfocado, qué ruta, qué selección, y qué pasa cuando un reescaneo cambia el árbol por debajo mientras el explorador está abierto.
- **Mover y redimensionar libremente la ventana queda fuera de la primera versión.** Se entrega centrada, grande y con tamaños preestablecidos. El valor está en poder bajar por la jerarquía sin escribir; arrastrar la ventana es acabado, y consume un tiempo desproporcionado respecto a lo que aporta.
- Dos vistas es más superficie que mantener que una. Se acepta a sabiendas: la alternativa era seguir sin poder llegar a un nodo concreto sin adivinar su nombre.
