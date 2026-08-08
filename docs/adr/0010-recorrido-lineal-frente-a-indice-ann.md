# ADR-0010 — Recorrido lineal en vez de `sqlite-vec`, y el umbral donde deja de valer

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0002](0002-sustrato-de-persistencia.md), [ADR-0009](0009-donde-viven-los-vectores.md)

## Contexto

§7.1 del maestro nombra `sqlite-vec` en la tabla de persistencia, con la coletilla «componente reemplazable». Al llegar a M4 hay que decidir si se usa.

Un índice ANN —`sqlite-vec`, HNSW, IVF— compra tiempo de consulta a cambio de tres cosas: una dependencia nativa que cargar, una estructura que mantener al insertar, y la posibilidad de **no devolver el vecino correcto**, que es lo que significa "aproximado".

La pregunta correcta no es si un ANN es más rápido en abstracto. Es si hay tiempo que ganar.

## Decisión

**Recorrido lineal completo con producto escalar. Sin `sqlite-vec` y sin ninguna extensión nativa.**

El corpus real son 2.479 vectores de 384 dimensiones: **3,63 MB**. La medición sobre esta máquina (i5-13420H, build release, mejor de 20 ejecuciones):

| | tiempo | por vector | techo a 50 ms |
|---|---|---|---|
| Recorrido leyendo de SQLite | **6,3 ms** | 0,00254 ms | ~19.700 vectores |
| Recorrido con la matriz ya en memoria | **0,57 ms** | 0,00023 ms | ~219.000 vectores |

Un índice aproximado no puede ganar los 0,57 ms que se están gastando, y sí añadiría las tres cosas de arriba.

### El hallazgo de la medición: el coste no está donde parecía

**El 91 % del tiempo se va en sacar los BLOB de SQLite y decodificarlos, no en el producto escalar.** Es la diferencia entre 6,3 ms y 0,57 ms, y cambia la respuesta a la pregunta siguiente.

Sin separar las dos cifras, el techo del recorrido lineal parece ser de ~19.700 vectores —unos 3.000 archivos, que este corpus alcanza en cuanto crezca— y la conclusión natural sería «hará falta un ANN pronto». Con las dos cifras separadas, la conclusión es la contraria: **cuando 6,3 ms estorbe, lo que hay que hacer no es aproximar, es dejar de deserializar.** Mantener la matriz residente cuesta 3,6 MB hoy y 154 MB con 100.000 vectores, y multiplica por diez el techo sin renunciar a la exactitud.

### El umbral, escrito

Con un presupuesto de **50 ms** para la rama vectorial —M2 estableció «instantánea» en 0,5–1,9 ms para BM25; 50 ms es el orden en el que una barra de búsqueda deja de sentirse inmediata, y deja margen para que las otras tres ramas de §6 quepan al lado—:

- **hasta ~19.700 vectores** — recorrido lineal leyendo de SQLite, que es lo que hay hoy;
- **hasta ~200.000 vectores** — recorrido lineal con la matriz residente en memoria;
- **por encima** — entonces sí toca medir un índice aproximado, y entonces `sqlite-vec` vuelve a la mesa.

200.000 vectores son del orden de 30.000 archivos con texto. El corpus de esta máquina tiene 394.

## Alternativas consideradas

### A. `sqlite-vec` ahora

La opción que el maestro dejaba nombrada, y por eso la que hay que descartar por escrito.

Rechazada porque a esta escala no acelera nada medible y cuesta: una extensión nativa que compilar o distribuir por plataforma, cargada en tiempo de ejecución con `load_extension` —que es una superficie de ataque que ahora mismo está cerrada—, y una estructura de índice que mantener. Además obliga a decidir si el almacenamiento vive en la extensión, lo que dejaría el esquema de I7 en manos de un tercero (ADR-0009, alternativa D).

Nada de esto dice que `sqlite-vec` sea mala; dice que es la respuesta a una pregunta que este corpus todavía no hace.

### B. Recorrido lineal con la matriz residente desde ya

Diez veces más rápido y ya está medido. Rechazada **por ahora** porque introduce estado vivo —una caché que hay que invalidar cuando cambian los vectores— para bajar de 6,3 ms a 0,57 ms, cuando ninguno de los dos números se nota. Es la primera optimización cuando haga falta, no antes, y este ADR existe para que llegado el momento no se pruebe primero con un ANN.

### C. Cuantizar los vectores para acelerar el recorrido

Aceleraría la parte que **no** es el cuello de botella, y pagaría con calidad. Rechazada por la medición.

### D. Filtrar candidatos con BM25 y vectorizar sólo esos

Convierte la rama vectorial en un reordenamiento de la léxica. Rechazada porque destruye el criterio de "hecho" del hito: encontrar por significado algo que BM25 **no** encuentra. Si la rama vectorial sólo ve lo que BM25 le pasa, no puede encontrar nada nuevo — que es exactamente lo que ocurre con `universidad`, donde BM25 devuelve dos archivos de código fuente y el documento correcto sólo lo trae el kNN.

## Consecuencias

**A favor**

- **Cero dependencias nativas por esta vía.** ADR-0002 eligió un archivo y cero instalación; esto no lo estropea.
- **Exacto.** El vecino más cercano es el vecino más cercano, no una aproximación con un parámetro de recall que ajustar. En un hito cuyo trabajo es juzgar la calidad del espacio vectorial, no tener que separar «el modelo se equivocó» de «el índice no lo encontró» vale más que los milisegundos.
- **El auditor existe.** Como todos los vectores caben en memoria, `--sample` puede calcular los *hubs* comparando todos contra todos, que es la única forma de ver la patología que ningún contador delata. Con un índice ANN ese análisis sería aproximado, es decir, inútil para lo que se usa.
- **El umbral está escrito y es comprobable.** `--medir` lo vuelve a calcular en cualquier máquina.

**En contra**

- **Se paga la deserialización en cada consulta.** 6,3 ms de los que 5,7 son leer y decodificar. Es tiempo tirado y está medido.
- **El coste crece linealmente y sin avisar.** No hay ninguna alarma que salte al pasar de 20.000 vectores. Debería haberla: el CLI enseña el número, pero nadie lo mira todos los días.
- **La medición es de una máquina, un corpus y una dimensión.** Con 1.024 dimensiones (`bge-m3`) los techos se dividen por 2,7 aproximadamente. El número que hay que rehacer al cambiar de modelo no es sólo la calidad.
- **`--sample` es O(N²).** Sobre 2.479 vectores son 6,1 millones de comparaciones y tarda segundos; sobre 200.000 serían 40.000 millones y dejaría de ser una herramienta interactiva. El auditor tiene un techo más bajo que el buscador, y es el auditor lo que hace confiable al buscador.
