# ADR-0016 — La prueba se elige; el índice no se filtra

- **Estado:** Aceptada
- **Fecha:** 2026-08-12
- **Cumple:** I9 · [`EXPERIENCE.md`](../EXPERIENCE.md) §4.4
- **Corrige el diagnóstico de:** [`PROJECT-MASTER.md`](../PROJECT-MASTER.md) §9, «Informe del primer corpus real», hallazgo 2

## Contexto

El informe del primer corpus real dejó escrito un problema del núcleo con este
título: *«El boilerplate de los PDF contamina los vectores y la procedencia»*.
La evidencia era real y grave: la consulta «comprobante de ingresos» —la
pregunta de la visa, literalmente— devolvía como prueba **un sello digital del
SAT en base64**, y los cinco primeros resultados empataban a la milésima.

El prompt de M5b lo puso como trabajo de este hito con una condición explícita:
*«no lo arregles filtrando por longitud o entropía sin medir primero»*. Es la
misma disciplina que M2 y M3 aprendieron con las cachés de Chromium — los
contadores decían éxito y auditar el contenido decía otra cosa.

**Se midió, y el diagnóstico documentado resultó ser el equivocado en sus dos
mitades.**

## Lo que se midió antes de decidir

Corpus real: 1.042 archivos, 4.058 fragmentos, 4.026 vectores, 598.499 tokens.

| Lo que decía el informe | Lo que dice la medición |
|---|---|
| El relleno «contamina los vectores» | Los sellos son **19 fragmentos de 4.058** (0,5 %) en 8 archivos. Dos de ellos ni son PDF: un `.env` y un `.ini`. No hay volumen que contaminar. |
| «El sistema acierta la clase y dentro de la clase no discrimina» | El fragmento ganador dice **«REPRESENTACIÓN IMPRESA DE UN CFDI»**, y CFDI es *Comprobante Fiscal Digital por Internet*. La recuperación acertó por la razón correcta. |
| Los cinco primeros empatan a 0.870 | **No es falta de discriminación: son los mismos bytes.** Comprobado par a par, 6 de 6 idénticos. `Estados_de_Cuenta_Abuelo_Visa.pdf` **contiene** los BNM de abril, mayo y junio; `Documentos_Identidad_Respaldo_Visa.pdf` **contiene** `EstadoCuenta5c0ae35f…pdf`. Cinco filas son tres documentos. |

Y el hallazgo que ninguna de las dos hipótesis contemplaba, que es el que
explicaba lo que se veía en pantalla:

> **La prueba eran los primeros 160 caracteres del fragmento ganador**
> ([`snippet_of`](../../core/obrain-embed/src/search.rs)). Nunca se buscaba la
> parte que había coincidido. La frase que explicaba el resultado estaba
> doscientos caracteres más abajo, **dentro del mismo fragmento**.

Dicho de otro modo: no había nada roto en la vectorización, ni en el troceado,
ni en la puntuación. Estaba roto el recorte. Filtrar el índice —lo que el
título del hallazgo invitaba a hacer— habría borrado estados de cuenta reales
sin mover un píxel de lo que se veía mal.

## Decisión

**1 · La prueba se elige, no se recorta.**

[`evidence::pick`](../../core/obrain-extract/src/evidence.rs) devuelve la
ventana del fragmento **centrada en donde aparece lo que buscaste**, saltando
el texto que no escribió una persona. A falta de coincidencia devuelve el tramo
de prosa más largo, que es el que más dice. El tramo sigue siendo una rebanada
exacta `[byte_start, byte_end)` del fragmento, con la misma propiedad que
[`chunk`](../../core/obrain-extract/src/chunk.rs) sostiene desde M2: una cita
apunta a un sitio real del documento, no a una copia parecida.

**2 · El texto de máquina no se enseña nunca, y no se borra nunca.**

Un sello sigue indexado, buscable y contando para la puntuación. Lo único que
no hace es hacerse pasar por una explicación. Esto es deliberado y va contra la
tentación obvia:

- Borrarlo del índice sería tirar dato por un problema de presentación, y I1
  dice que el índice es una proyección de la verdad, no un sitio donde curar
  la verdad.
- Si algún día alguien busca el folio fiscal literal, tiene que encontrarlo.
- Y no había volumen que ganar: 0,5 %.

**3 · La señal está medida, y las tres candidatas descartadas quedan escritas
para que nadie las vuelva a proponer.**

| Candidata | Por qué no |
|---|---|
| Tirada larga de alfabeto base64 | La extracción del PDF mete espacios y saltos **dentro** del sello. No hay tirada que encontrar. Fue la primera medición y dio 2 fragmentos: un falso negativo casi total. |
| Tokens de ≥40 caracteres | Se lleva URLs y correos (`centrodeatencioneninversiones@banamex.com`). 8.847 tokens del corpus, y un correo dentro de un documento es prueba legítima. |
| Proporción de caracteres sin vocal | Se lleva código fuente y los PDF de ejercicios de programación. |

La que queda: **un token de ≥40 caracteres, del alfabeto base64 y con ≥12
caracteres distintos**. Marca 69 tokens de 598.499, en 7 archivos. Más una
regla de **vecindad** para la cola del sello —la última línea de un bloque
partido en líneas de 80 es un resto corto que ninguna regla global puede cazar
sin comerse palabras—, que sólo se aplica pegada a un sello ya confirmado.

**4 · La asimetría es a propósito.** Equivocarse marcando texto humano como
máquina cuesta una ventana peor. Equivocarse al revés enseña un sello en el
gesto que más confianza tiene que generar. Ante la duda se descarta el tramo, y
si no queda nada legible se enseña **vacío**: un hueco es honesto; un sello
disfrazado de explicación, no. Medido sobre el corpus entero, se queda sin
prueba **1 fragmento de 4.058**.

**5 · El empate no se rompe aquí, porque no es un fallo de puntuación.**

Que un documento esté dentro de otro es un hecho del grafo, no ruido del
índice. M3 ya modela `duplicate_of` para archivos enteros (44 en este corpus);
lo que no existe es *«este documento está contenido en aquel»*. Resolverlo
colapsaría cinco filas en tres **y diría por qué**, que es exactamente lo que un
gestor de archivos no puede hacer y este proyecto sí. Queda como pendiente
estructural en §10 del maestro, no como deuda de esta decisión.

## Alternativas consideradas

### A. Filtrar los fragmentos de sello del índice al trocear

Es lo que sugería el título del hallazgo original. Rechazada por la medición:
son el 0,5 % del índice, así que no arregla ningún problema de volumen, y no
toca el recorte, que era lo que se veía mal. Además obligaría a subir la
versión del chunker (I7) y a reconstruir los vectores del corpus entero para
arreglar un problema de presentación.

### B. Deduplicar por hash de texto y no vectorizar lo repetido

Habría quitado el 11,5 % del índice de una vez, y es la alternativa que más
tentadora parecía antes de mirar el contenido. Rechazada porque **el texto
repetido no es relleno: es contenido real que vive en dos archivos**. Quitarlo
haría que el PDF recopilatorio dejara de ser encontrable justo por las partes
que lo hacen valioso, y el usuario no tendría forma de saber por qué.

### C. Marcar el sello en el momento de extraer, con una columna en `chunks`

Más rápido en consulta que recalcularlo al enseñar. Rechazada por ahora porque
congela la regla en los datos: cambiarla obligaría a reindexar. Recalcular
cuesta microsegundos sobre un fragmento de 1 KB y sólo se hace sobre los que se
van a enseñar. Si algún día el coste aparece en una medición, esto se
reconsidera con el número delante.

## Consecuencias

**A favor**

- La pregunta de la visa enseña, en sus diez resultados, texto en español que
  explica por qué está cada uno. Antes los dos primeros eran base64.
- La animación de **Trazo** —la que §3 llama *la animación que vende el
  producto*— y el panel de entidad de §4.4 se apoyan los dos en este tramo. Se
  construyen ahora sobre algo que se puede leer.
- `obrain-extract --pruebas` queda en el núcleo. La próxima vez que alguien
  quiera tocar esto, mide antes — como `--sample` en M2 y `--pesos` en M5a.
- No se tiró ni un dato, así que no hay nada que reconstruir ni ninguna versión
  de chunker que subir.

**En contra**

- **El coste se paga en cada consulta**, no una vez al indexar. Es la
  alternativa C invertida, y se acepta a sabiendas por poder cambiar la regla
  sin reindexar. Está sin medir en un corpus grande.
- **La regla es de este corpus.** 40 caracteres y 12 distintos separan bien
  sellos del SAT de palabras en español; una firma PGP en ASCII armor, un
  bloque MIME o un corpus en alemán pueden comportarse de otro modo. Es la
  misma objeción que [ADR-0015](0015-el-cielo-reparte-por-anchura-no-por-peso.md)
  se puso a sí mismo con √regiones, y sigue siendo válida.
- **Siete claves de un `.ini` de configuración caen dentro de la regla.** Se
  puede discutir si son sellos; no se puede discutir que las escribió una
  máquina. Se asume.
- **El empate sigue ahí.** Se ve mejor que antes —las cinco filas ahora se
  explican— pero siguen siendo cinco filas para tres documentos, y eso no lo
  arregla ninguna regla de presentación.
