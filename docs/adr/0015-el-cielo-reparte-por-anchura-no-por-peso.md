# ADR-0015 — El cielo reparte por anchura; el tamaño sigue siendo cuántos archivos hay

- **Estado:** Aceptada
- **Fecha:** 2026-08-10
- **Complementa a:** [ADR-0013](0013-el-mapa-es-la-pantalla.md)

## Contexto

El primer corpus real (informe en [`PROJECT-MASTER.md`](../PROJECT-MASTER.md) §9) dejó dos quejas contra el cielo, y durante un rato se trataron como una sola:

1. **Un único `whisper.cpp` de terceros se llevó 21 de las 30 regiones**, mientras `OneDrive` —donde estaba la vida entera del usuario— se quedaba en un solo grumo de 851 archivos sin abrir.
2. **La región mayor eran 229 capturas de pantalla.** Las resoluciones del IMSS y los estados del banco —la pregunta de la visa, literalmente— caían en regiones de 12 y de 2.

Son dos problemas distintos. El primero es **cómo se reparten las plazas**; el segundo es **qué significa el tamaño**. `sky::regions` los tenía conflados porque el mismo número, el peso del subárbol, hacía las dos cosas.

## Lo que se midió antes de decidir

Se añadió `obrain-api --pesos` ([`sky::audit`](../../core/obrain-api/src/sky.rs)) siguiendo la convención de `--sample` de M2, M3 y M4: auditar es parte del hito, no una herramienta aparte. Seis señales sobre las 29 regiones del corpus real.

| Señal | Resultado |
|---|---|
| **archivos** | La regla de hoy. |
| **aserciones** | Correlación casi perfecta con archivos. No añade información. |
| **sesiones** | Igual, con un acantilado tras las dos primeras (30, 27, luego ≤7). |
| **aserciones por archivo** | Mide *«¿esto es texto o es binario?»*. Gana `ClipOps/.claude`. |
| **aristas entre regiones** | **Inutilizable**: el máximo son 3. Las aristas deterministas entre regiones aún no existen en cantidad. |
| **lapso temporal** | **La única que reordena de verdad.** |

Por lapso, `OneDrive/Documentos` —donde viven el IMSS y el BBVA— sube del puesto 15 al 2 (1.836 días, cinco años), y las capturas caen del 1 al 7 (222 días). Es la corrección que se buscaba.

**Y aun así no sirve.** El primero por lapso es `Documentos/Clansoft`: 7 archivos y 3.210 días. Por lapso, siete archivos viejos se convierten en la estrella mayor del cielo. El lapso corrige el sesgo del volumen y mete el suyo.

## Decisión

**1 · El tamaño no cambia. Una región mide lo que mide su número de archivos.**

Ninguna de las seis señales mide importancia, porque **la importancia no es un dato que esté en el disco**. Combinarlas en una puntuación sería exactamente lo que [`EXPERIENCE.md`](../EXPERIENCE.md) §1 prohíbe —*«nunca importancia arbitraria: eso sería una opinión disfrazada de dato»*— y una media de cinco cosas es una opinión con mejor disfraz, no con menos disfraz.

Contar archivos es honesto: dice cuánto hay, y no pretende decir qué importa.

**2 · El reparto sí cambia, porque ahí sí había un fallo.**

El fallo no era el que parecía. No era bajar codiciosamente por la rama más pesada: era que **un solo paso promovía todas las hijas de golpe**. El techo era «las que quepan en el presupuesto», y al principio caben casi todas, así que `whisper.cpp` soltaba diecinueve estrellas en una única operación. El comentario del propio código ya describía la intención correcta —*«unas cuantas ciudades con nombre y el campo alrededor»*— y la implementación no la cumplía.

Tres cambios, y los tres hacen falta:

| Cambio | Qué arregla |
|---|---|
| **Techo de √regiones por paso** | Una carpeta con veinte hijas aporta un puñado de estrellas, no veinte. |
| **El peso se divide por el nivel** | Bajar otra vez por donde ya bajaste exige el doble de peso, luego el triple. Una rama honda sigue pudiendo abrirse, pero deja de hacerlo gratis. |
| **El resto vuelve a competir** | Sin esto el techo entierra lo que no cupo. Se detectó con el test de los 100 archivos: salían 4 regiones donde tenían que salir 10. Un resto pesa lo suyo **más lo que aún no ha soltado**. |

**3 · La respuesta a «qué es importante» es que lo diga el usuario, no una fórmula mejor.**

Pedido el 2026-08-10: *«quiero poder ordenar mis cosas a mi gusto y que el programa no pierda de vista qué es qué y con qué se relaciona»*. El invariante **I5** ya garantiza que una aserción del usuario gana para siempre sobre cualquier inferencia, y `Brain::correct` ya la escribe con test. Que el usuario marque *«esto es mi trámite de visa»* resuelve el problema del tamaño **sin que el sistema tenga que adivinar nada**. Queda como pendiente estructural en §10 del maestro; esta ADR sólo deja constancia de que es la salida, no un extra.

**4 · Se revisa en M6.** [ADR-0013](0013-el-mapa-es-la-pantalla.md) ya lo dejó escrito: *«una carpeta no es un núcleo de la vida de nadie»*. Eso no lo arregla una fórmula; lo arregla detectar `Process`.

## Lo que se midió después

Sobre el corpus patológico (3.634 archivos, `whisper.cpp` incluido):

| | Antes | Después |
|---|---:|---:|
| Regiones de `whisper.cpp` | 21 de 30 | **15 de 30** |
| `OneDrive` | 1 grumo sin abrir, 851 archivos | **4 regiones**: Documentos 594, Imágenes 236, Escritorio 18, Datos adjuntos 2 |
| Región nº 1 del cielo | `ggml/src` | **`OneDrive/Documentos`** |

Sobre el corpus real (1.042 archivos), el cielo queda prácticamente igual: no había nada que arreglar ahí, y era importante comprobar que no se rompía.

**Las 15 regiones que `whisper.cpp` conserva no son un defecto.** Ese repositorio es el 52 % de los archivos de ese corpus; que se lleve la mitad del cielo es lo que cualquier regla honesta basada en volumen tiene que decir. Lo que se arregló es la **inanición**: que un hermano con 851 archivos no se abriera ni una vez. La solución al resto no es el algoritmo, es no escanear un checkout de terceros — y eso ya lo resolvió la señal de exclusiones del mismo informe.

## Consecuencias

**A favor**

- La primera región del cielo pasa a ser la carpeta donde vive el trámite de visa, y no un directorio de fuentes de terceros.
- El reparto ya no depende del orden de llegada: una rama que quiera más plazas tiene que ganárselas compitiendo en cada vuelta.
- `--pesos` queda en el núcleo. La próxima vez que alguien quiera cambiar el dimensionado, mide primero.
- No se inventó ninguna puntuación de importancia, así que no hay nada que desmontar cuando M6 traiga `Process`.

**En contra**

- **El cielo puede bajar más de nivel que antes.** Con el corpus patológico aparecieron regiones a profundidad 6 (`src/ggml-cuda`, `src/ggml-vulkan`): el techo por paso obliga a más pasos, y más pasos es más profundidad. Con corpus sanos no se ha observado, pero es el precio del cambio y conviene vigilarlo.
- **√regiones es una curva elegida por su forma, no medida.** Es exactamente la misma objeción que ADR-0013 se puso a sí mismo con `effective`, y sigue vigente: con corpus mayores habrá que revisarla.
- El tamaño sigue sin significar importancia, y eso seguirá chirriando hasta M6. Está asumido a sabiendas: es preferible a que signifique una opinión.
