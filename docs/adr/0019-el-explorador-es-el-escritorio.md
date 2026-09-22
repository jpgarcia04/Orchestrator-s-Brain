# ADR-0019 — El explorador es el escritorio; el mapa es una vista

- **Estado:** Aceptada
- **Fecha:** 2026-09-22
- **Sustituye a:** [ADR-0013](0013-el-mapa-es-la-pantalla.md)
- **Acota:** [ADR-0014](0014-dos-vistas-un-solo-estado.md) (sus puntos 1 a 3)
- **Conserva:** el argumento de teclado de [ADR-0004](0004-punto-de-entrada-barra-no-galaxia.md) y [ADR-0017](0017-la-barra-se-pliega.md)

## Contexto

[ADR-0013](0013-el-mapa-es-la-pantalla.md) decidió que **el mapa es la
pantalla**: al abrir se ve el cielo y la barra busca dentro de él. Tenía razón
para el proyecto de agosto, que quería entender *todo el disco* del usuario. En
un disco entero, las carpetas son en su mayoría ruido del sistema y de las
herramientas, y lo único que da forma a ese paisaje es agregarlo por escala.

El 2026-09-22 el proyecto se replanteó ([ADR-0018](0018-bibliotecas.md)): ya no
es *«todo mi disco»* sino **una bóveda de documentos personales que el usuario
elige uno a uno** —pasaporte, pólizas, matrícula, contratos— y ordena él mismo
(`01 Identidad`, `02 Salud`, `03 Estudios`…). Eso cambia dos cosas que 0013 no
podía conocer:

1. **El árbol de carpetas ya no es ruido: es el orden del usuario.** Una carpeta
   `01 Identidad` dice exactamente lo que es. El explorador deja de ser «el
   explorador de Windows con pasos de más» y pasa a ser la forma más directa de
   ver lo que la persona misma organizó.
2. **La tarea diaria es concreta y corta.** *Necesito la póliza del seguro*:
   buscarla, verla, abrirla. Es la tarea de las cuarenta veces al día de
   [ADR-0004](0004-punto-de-entrada-barra-no-galaxia.md), y con una bóveda
   personal no hay nada que descubrir en el cielo para hacerla.

La Fase 2 se construyó con la propuesta (§10 del maestro, 2026-09-22): el
explorador como escritorio y la galaxia como vista «Mapa», con el aspecto de
noche por defecto. **El usuario revisó las capturas y la confirmó** ese mismo
día, con sus palabras: *«Vi las capturas de la Fase 2 y me gustaron: el
explorador como pantalla de inicio, la galaxia como vista «Mapa» y el aspecto de
noche por defecto (el de día sigue a un clic). No le muevas al diseño»*.

## Decisión

**El explorador es el escritorio de trabajo, y la galaxia es una vista propia e
interactiva a la que se va para explorar.**

1. **Al abrir, el Inicio.** Un escritorio estilo sistema operativo: barra
   lateral con tus carpetas y las vistas, un Inicio con lo último que tocaste,
   tus carpetas y tu mapa en pequeño. Con la bóveda vacía, lo único que enseña
   es cómo empezarla («Elegir carpeta»).
2. **El mapa es la vista «Mapa»**, en la barra lateral, con la galaxia de
   siempre (PixiJS, `sky::regions`, puentes, la cámara de M5d). Sigue siendo
   interactiva y **está enlazada en los dos sentidos**: pulsar una estrella abre
   esa carpeta en el explorador, y «Ver en el mapa» lleva de un documento o una
   carpeta a su vecindario en la galaxia. En el Inicio aparece en pequeño, como
   una tarjeta que lleva a ella.
3. **Buscar no depende de dónde estés.** `Ctrl K`, o empezar a escribir en
   cualquier sitio: la primera letra abre el buscador y no se pierde
   ([ADR-0017](0017-la-barra-se-pliega.md)). Los resultados siguen saliendo
   agrupados por la carpeta que comparten (`sky::cluster`), que era la mitad
   medible de 0013 y no se toca.
4. **Un clic enseña, doble clic abre.** Un clic en un documento lo pone en la
   vista previa de la derecha sin moverte de donde estás; doble clic lo abre
   con su programa. Lo sensible sale borroso hasta pulsar «Mostrar».
5. **Noche por defecto, día a un clic.** El tema se recuerda por equipo
   (`obrain.tema`); son variables CSS en `ui/src/estilos/tema.css` y cambiar de
   uno a otro no cambia ninguna disposición.

### Qué sobrevive de ADR-0013 y qué cae

| De 0013 | Qué pasa |
|---|---|
| Los resultados salen agrupados por el concentrador que comparten | **Sobrevive**, en el buscador |
| `sky::regions`: el cielo por escala | **Sobrevive**: es la vista «Mapa» y la tarjeta del Inicio |
| Los puentes entre regiones | **Sobreviven**, en el mapa |
| El camino de teclado desde el primer instante | **Sobrevive**, y ahora en todas las vistas |
| *Al abrir se ve el cielo* | **Cae**: al abrir se ve el Inicio |
| *La barra busca dentro del mapa y el mapa se enciende* | **Cae como pantalla principal**: buscar es un buscador propio; «Ver en el mapa» es el camino a la galaxia |

### Qué acota de ADR-0014

Sus puntos 1 a 3 (*la galaxia es el estado base*, *el explorador siempre se
invoca*, *al entrar al explorador la galaxia se congela detrás*) quedan
invertidos: ahora el explorador es la base y el mapa se invoca. Siguen en pie
sus puntos 4 a 6 —**continuidad de estado en los dos sentidos**, que el
explorador enseñe lo que un gestor de archivos no puede, y que §4.4 de
[`EXPERIENCE.md`](../EXPERIENCE.md) se acota y no se deroga—, porque no
dependían de cuál de las dos vistas estuviera debajo.

## Alternativas consideradas

### A. Mantener ADR-0013: el mapa como pantalla

Rechazada por el usuario tras ver las dos. Con una bóveda personal, el cielo al
abrir enseña una docena de estrellas con nombres que la persona ya conoce
porque los escribió ella, y obliga a pasar por ellas para llegar a un
documento. Es exactamente el peaje diario que ADR-0004 existía para evitar.

### B. El explorador como pantalla, y quitar la galaxia

Rechazada por el usuario: *la galaxia sigue siendo interactiva*. Además no
cuesta nada conservarla —el núcleo que la dibuja ya existe y es el mismo que
agrupa los resultados— y es donde vivirán los trámites cuando llegue M6.

### C. Día por defecto

Se le enseñaron los dos aspectos y eligió la noche, que además es la dirección
de [`EXPERIENCE.md`](../EXPERIENCE.md) §1 (*«un observatorio de noche»*). El de
día queda a un clic y recordado.

## Consecuencias

**A favor**

- La aplicación abre donde se trabaja: el camino de las cuarenta veces al día
  no pasa por ninguna animación (I10).
- El orden que el usuario da a su bóveda se ve tal cual, con nombres limpios
  ([ADR-0018](0018-bibliotecas.md)).
- Nada del núcleo se tira: el cielo, la lente, los puentes y los grupos siguen
  siendo las mismas piezas; sólo cambia cuál se ve primero.
- El mapa en reposo no gasta nada: sólo se dibuja mientras la vista «Mapa» está
  abierta, y el lienzo se destruye al salir.

**En contra**

- **El riesgo que 0013 y 0014 dejaron escrito se hace más probable, no menos.**
  Si el explorador es lo cómodo y la galaxia lo bonito, el producto será un
  gestor de archivos con salvapantallas. La respuesta sigue siendo la misma: que
  el explorador enseñe lo que un gestor de archivos no puede (lo inferido con su
  «¿por qué?», lo sensible, los vencimientos de la Fase 3), y que la galaxia se
  gane su uso cuando M6 dibuje trámites en vez de carpetas.
- **Dos vistas enlazadas siguen siendo dos superficies que mantener**, con los
  bugs donde siempre: en el estado compartido (qué está abierto, qué está
  seleccionado) cuando un reindexado cambia el árbol por debajo.
- **Nunca se ha probado en Windows de verdad.** La confirmación se dio sobre
  capturas del banco de pruebas (Chromium con un puente HTTP), no sobre WebView2.
  Si en Windows algo se ve distinto, se arregla sin reabrir esta decisión.
