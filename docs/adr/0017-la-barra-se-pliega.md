# ADR-0017 — La barra se pliega a un icono, y escribir la despliega

- **Estado:** Aceptada
- **Fecha:** 2026-08-14
- **Complementa a:** [ADR-0013](0013-el-mapa-es-la-pantalla.md)
- **Conserva el argumento de:** [ADR-0004](0004-punto-de-entrada-barra-no-galaxia.md)

## Contexto

Dos ADRs han decidido ya dónde va la barra de comandos, y los dos comparten una misma premisa que ninguno ha puesto en duda:

- [ADR-0004](0004-punto-de-entrada-barra-no-galaxia.md) la puso delante del mapa. Su argumento: *recuperar es una tarea de teclado que se hace cuarenta veces al día, comprender es una tarea de ratón que se hace una vez por semana, y poner la segunda delante de la primera cobra un peaje cada día.*
- [ADR-0013](0013-el-mapa-es-la-pantalla.md) lo sustituyó: **el mapa es la pantalla y la barra busca dentro de él**. No derogó el argumento del peaje —lo dio por resuelto, porque el mapa ya no interrumpe: *es* donde se busca.

La premisa común es que **el camino de las cuarenta veces al día no puede costar un gesto de más**. Sigue intacta y esta decisión no la toca.

Lo que sí ha cambiado es que la aplicación se ha usado. El veredicto, literal: *«la barra de búsqueda como que estorba un poco, quizás más esquinada o sólo un icono, estaría bien evaluarlo»*.

Y hay un hecho nuevo que ninguno de los dos ADRs podía conocer, porque llegó con M5d: **el mapa ahora se maneja**. Hasta ayer el centro de la pantalla era un dibujo, y 560 px flotando encima tapaban una imagen. Con cámara, el centro de la pantalla es la superficie por la que se arrastra y sobre la que se hace zoom, y esos 560 px pasan a estar sobre el sitio donde se trabaja.

## Decisión

**La barra se pliega a un icono en la esquina superior izquierda, y cualquier tecla imprimible la despliega.**

1. **Plegada es un icono** de 44 px. El mapa se ve entero.
2. **Escribir la abre, y la primera letra no se pierde**: entra como primer carácter de la consulta. Es la parte que hace que esta decisión no contradiga a ADR-0004 — el camino de teclado sigue siendo *escribir*, sin un gesto delante.
3. **Se repliega sola** al salir de ella vacía. Nunca se repliega con algo escrito dentro: eso sería tirar lo que el usuario acaba de teclear.
4. `Esc` deshace un escalón cada vez: primero lo escrito, luego la barra, luego el panel, luego la cámara, luego el cielo.
5. **La línea de estado sale de la barra** y pasa a la esquina inferior izquierda, siempre visible. Es la línea del mapa, no la de la búsqueda: dice cuántas regiones y cuántos archivos hay al arrancar, y qué se acaba de encontrar al buscar.

## Alternativas consideradas

### A. Dejarla como estaba

Rechazada por el usuario tras usarla, y ahora también por el hecho nuevo: con cámara, el centro de la pantalla deja de ser un fondo y pasa a ser la superficie de trabajo.

### B. Estrecha y anclada arriba, siempre visible

Se le ofreció junto con las otras dos y no la eligió. Es la opción conservadora —no cambia ningún gesto— pero ocupa una esquina de forma permanente para algo que la mayoría de los segundos no se está usando.

### C. Sólo un icono, sin la tecla

**Ésta es la que había que rechazar con cuidado, porque es la trampa.** Un icono que hay que pulsar antes de escribir añade un gesto al camino de las cuarenta veces al día, que es exactamente lo que ADR-0004 existía para proteger. Un cambio que parece de disposición y que en realidad cobra un peaje diario es el error que este proyecto ya cometió una vez y corrigió por escrito.

## Consecuencias

**A favor**

- Refuerza ADR-0013: el mapa es la pantalla, y ahora lo es entera.
- El camino de teclado sigue costando cero gestos, que es la premisa que los dos ADRs anteriores comparten.
- Desaparece una coordinación: la barra ya no tiene que apartarse cuando se abre el panel lateral, porque vive en la esquina contraria.

**En contra**

- **Al arrancar no hay un campo visible donde escribir.** Se descubre por el icono, y quien no lo pruebe puede tardar en enterarse de que basta con teclear. Es el coste real de plegarla y se acepta a sabiendas: la alternativa era tapar el mapa el 100 % del tiempo para anunciar algo que se usa el 5 %.
- **La primera letra se captura en un manejador global de teclado.** Eso reserva de hecho todas las teclas sueltas: cualquier atajo futuro de una sola tecla competiría con escribir. Hoy no hay ninguno —`Ctrl+E` lleva modificador— pero la tecla global del escalón 1 de [`EXPERIENCE.md`](../EXPERIENCE.md) §4.1 tendrá que llevarlo también.
- La barra sigue siendo el punto de entrada aunque no lo parezca. Si algún día el icono resulta que nadie lo encuentra y nadie descubre que escribir funciona, el fallo no será el pliegue: será que no se enseñó una vez.
