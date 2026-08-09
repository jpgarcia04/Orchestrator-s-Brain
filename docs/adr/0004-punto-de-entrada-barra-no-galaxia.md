# ADR-0004 — La barra de comandos es el punto de entrada; el mapa es el sustrato

- **Estado:** **Sustituida** por [ADR-0013](0013-el-mapa-es-la-pantalla.md) (2026-08-09)
- **Fecha:** 2026-08-07

> **Por qué se sustituyó.** Se implementó en M5a y, con la aplicación delante, no
> se sostuvo. Sobrevive la mitad buena —el camino de teclado y que el mapa
> reaccione mientras se escribe—; cae que el mapa fuera un fondo. Y la causa
> resultó no ser la disposición: faltaba **agregación por escala**, así que no
> había con qué dibujar un mapa que mereciera la pantalla. El razonamiento de
> abajo se conserva entero porque su análisis de las dos tareas sigue siendo
> correcto y es lo que ADR-0013 hereda.

## Contexto

La visión original pedía explícitamente que la pantalla principal **no** fuera un explorador de archivos sino "un universo, un mapa mental, una galaxia". Esa identidad visual es el diferenciador del producto y no se discute.

Pero la misma visión pedía también que "el usuario nunca debería preocuparse por dónde guardó algo". Ambas cosas son ciertas y apuntan a interfaces distintas, porque **son tareas distintas**:

- **Recuperar** — sé lo que quiero, lo quiero ya. Unas 40 veces al día. Teclado.
- **Comprender** — no sé exactamente qué busco, quiero ver cómo se relaciona. Semanal. Ratón.

Si el mapa es la pantalla de inicio, se interpone entre el usuario y su trabajo 40 veces al día. Es exactamente el motivo por el que el *graph view* de Obsidian es una captura de pantalla que la gente enseña y luego no usa.

## Decisión

**La barra de comandos es el punto de entrada**, invocada con una tecla global sobre cualquier aplicación.

**El mapa no es otra pantalla: es el sustrato sobre el que flota la barra.** Se dibuja detrás, vivo y atenuado, y reacciona mientras el usuario escribe: las regiones coincidentes se encienden débilmente.

La transición entre recuperar y comprender es un **gradiente**, no un salto entre vistas.

## Alternativas consideradas

### A. La galaxia como pantalla de inicio (la petición literal)

Rechazada. Máximo impacto en la primera sesión y coste creciente en todas las demás. Además agrava el arranque en frío: es precisamente al principio, cuando el grafo está vacío, cuando el mapa se ve peor y cuando se le daría más protagonismo.

### B. Dos aplicaciones separadas, o dos pestañas

Rechazada. Separar recuperar de comprender rompe la promesa central: que todo es un mismo ecosistema. Y obliga al usuario a decidir *qué tipo de tarea* está haciendo antes de empezar, que es justo la carga cognitiva que el producto existe para eliminar.

### C. Barra de comandos sola, mapa como función secundaria en un menú

Rechazada por el motivo opuesto: convierte el diferenciador del producto en una curiosidad enterrada. Si el mapa no se ve a diario, no se aprende; si no se aprende, no se usa nunca.

## Consecuencias

**A favor**

- El camino de 40 veces al día es instantáneo y de teclado. La app se gana el uso diario.
- El mapa se aprende de forma **periférica**, sin exigir que nadie lo estudie.
- La identidad visual está presente en cada interacción, incluso en las de dos segundos.
- Suaviza el arranque en frío: durante las primeras semanas, un mapa disperso se ve como un fondo atmosférico, no como un fracaso.

**En contra**

- **Contradice la petición literal del usuario.** Aceptada explícitamente tras argumentarla, pero queda registrado que fue una desviación consciente de la visión original.
- Renderizar un mapa vivo detrás de la barra tiene un coste continuo de GPU, que es lo que obliga al modo reposo (§2 de EXPERIENCE.md).
- El mapa se ve más veces de reojo que de frente, así que tiene que ser legible **desenfocado y atenuado**. Es una restricción de diseño fuerte y poco habitual.
