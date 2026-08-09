# ADR-0013 — El mapa es la pantalla; la barra busca dentro de él

- **Estado:** Aceptada
- **Fecha:** 2026-08-09
- **Sustituye a:** [ADR-0004](0004-punto-de-entrada-barra-no-galaxia.md)

## Contexto

[ADR-0004](0004-punto-de-entrada-barra-no-galaxia.md) decidió que la barra de comandos fuera el punto de entrada y el mapa el sustrato atenuado del fondo. El argumento era bueno y sigue siéndolo en su parte: recuperar es una tarea de teclado que se hace cuarenta veces al día, comprender es una tarea de ratón que se hace una vez por semana, y poner la segunda delante de la primera cobra un peaje cada día.

Ese ADR dejó escrito, en sus propias consecuencias:

> **Contradice la petición literal del usuario.** Aceptada explícitamente tras argumentarla, pero queda registrado que fue una desviación consciente de la visión original.

M5a la implementó. Con la aplicación delante, el veredicto fue que no: la pantalla se ve genérica, no es la galaxia de la visión, y —lo más concreto— **una consulta devuelve cincuenta archivos en fila cuando lo que hay de verdad son tres o cuatro sitios**.

El párrafo de arriba existía para este momento. Lo que sigue no es un cambio de opinión estético: es que al construirlo aparecieron dos hechos que el ADR anterior no podía conocer.

**Hecho 1: el problema de los cincuenta resultados no era de la interfaz.** El sistema ya sabía agruparlos. M3 lleva desde el tercer hito creando nodos concentradores —carpetas y sesiones— y la superficie de búsqueda los ignoraba, devolviendo archivos planos. Ninguna disposición de pantalla arregla eso.

**Hecho 2: no había con qué dibujar una galaxia.** La única lente que existía era foco + radio (I2), que responde *«esto y lo que tiene al lado»*. Un vecindario no es un cielo. Lo que hace falta —y lo que [`EXPERIENCE.md`](../EXPERIENCE.md) §6 ya pedía por su nombre, llamándolo una de las dos cosas que hay que diseñar *ahora*— es agregación por escala: *«a poco zoom no ves 40.000 nodos: ves 30 regiones con nombre»*.

Dicho de otro modo: ADR-0004 resolvió una tensión de *disposición* cuando la carencia era de *agregación*.

## Decisión

**El mapa es la pantalla.** Al abrir se ve el cielo: las regiones del grafo con su nombre, las grandes más brillantes.

**La barra busca dentro de él**, y sigue estando en el camino de teclado desde el primer instante. Al escribir, el mapa **no cambia de escala: se enciende** por donde caen los resultados. Es la analogía que el usuario trajo y que resultó ser exacta: un mapa de calles. Ves el mapa; si buscas una dirección, el mapa se ilumina donde está; si te acercas, se despliega.

**Los resultados salen agrupados** por el nodo concentrador que comparten, no en fila.

Tres piezas nuevas en el núcleo, ninguna en la interfaz:

| Pieza | Qué añade |
|---|---|
| `sky::regions` | Parte el árbol de contención por la región mayor, repetidamente, hasta el número pedido. Es el zoom. |
| `sky::cluster` | El mismo árbol leído al ras: los resultados agrupados por carpeta. |
| Puentes | Una sesión que cruzó dos regiones no es un nodo: es una **arista entre regiones**. |

## Alternativas consideradas

### A. Mantener ADR-0004 y sólo agrupar los resultados

Habría arreglado la mitad medible del problema —los cincuenta archivos en fila— sin tocar la disposición. Rechazada porque deja intacto lo que motivó el proyecto: la visión pedía un mapa del conocimiento, no un buscador con un fondo bonito. Y porque el argumento del peaje diario se debilita con la corrección: el mapa ya no interrumpe, **es** donde se busca.

### B. El mapa como pantalla, y la barra en un menú

Rechazada por el mismo motivo por el que ADR-0004 la rechazó en su día, y ese motivo no ha cambiado: el camino de las cuarenta veces al día tiene que ser de teclado y estar disponible sin buscarlo.

### C. Esperar a M6 y dibujar `Process` en vez de carpetas

Los «núcleos de tu vida» de la visión son, propiamente, entidades `Process` —un trámite, un viaje, un proyecto— y eso es M6. Rechazada porque hay una versión determinista disponible hoy: las carpetas y las sesiones que M3 ya escribió. Esperar significaba no poder juzgar la experiencia hasta dos hitos después, y las regiones de carpeta son un sustituto honesto que además no hay que tirar: cuando M6 detecte procesos, serán regiones de otra clase en el mismo cielo.

## Consecuencias

**A favor**

- La primera pantalla dice algo del usuario en vez de estar en blanco. §5 pedía que el arranque en frío nunca fuera un estado quieto.
- Agrupar resultados usa estructura que ya existía desde M3 y que nadie leía.
- Los puentes convierten las sesiones de trabajo en la relación que `EXPERIENCE.md` §6 llama «Puentes», sin inventar ninguna arista (I3 intacto).
- La agregación por escala queda hecha, y era una de las dos cosas que §6 exigía diseñar antes de que el corpus creciera.

**En contra**

- **El coste de GPU que ADR-0004 quería evitar sigue ahí y ahora es más visible.** Se sostiene por el mismo motivo que entonces: el bucle de render está parado y sólo se dibuja al cambiar de escala.
- **Las regiones son carpetas, y una carpeta no es un núcleo de la vida de nadie.** Es una aproximación honesta y provisional. Si M6 no llega a detectar procesos, el cielo se quedará siendo un explorador de archivos bonito — que es exactamente lo que este proyecto no quiere ser.
- El número de regiones se adapta al tamaño del corpus por una raíz cuadrada elegida por su curva, no medida. Con corpus reales habrá que revisarla.
- La calidad del cielo depende por completo de cómo tenga el usuario organizadas sus carpetas. Para quien lo tenga todo en `Descargas`, el cielo será una estrella.
