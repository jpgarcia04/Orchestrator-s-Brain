# ADR-0005 — El texto extraído es una proyección, no una aserción

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0001](0001-log-de-aserciones-como-fuente-de-verdad.md), [ADR-0002](0002-sustrato-de-persistencia.md)

## Contexto

M2 extrae el texto de los archivos, lo fragmenta y lo indexa. Eso plantea una pregunta que ADR-0001 no contestó porque en M0 no había contenido: **¿el texto de un documento es una aserción sobre ese documento, o es un índice?**

No es una discusión académica. De la respuesta dependen tres cosas caras:

1. Si el texto es una aserción, es **inmutable** (I1) y **no se borra** (I4). Reextraer un archivo editado deja las dos versiones del texto en el log para siempre.
2. El invariante I7 exige que todo vector guarde `modelo + versión + dimensión + chunker`. Si los fragmentos no tienen identidad propia y versionada, M4 no puede migrar de modelo de embeddings sin reindexar a ciegas.
3. FTS5 necesita una tabla real que indexar. Si el texto vive dentro de `assertions.object_text`, hay que duplicarlo igualmente para el índice.

## Decisión

**El texto extraído y sus fragmentos viven en tablas propias (`extractions`, `chunks`, `chunks_fts`), fuera del log. Al log va sólo el hecho de haber extraído.**

Concretamente:

| Dónde | Qué | Por qué |
|---|---|---|
| `chunks` | fragmento, `(byte_start, byte_end)`, `chunker`, `chunker_ver` | Es una **derivación** del archivo. Se puede tirar y rehacer. |
| `chunks_fts` | índice FTS5 con `content='chunks'` | Proyección de una proyección. El maestro (§7.1) ya lo llamaba proyección. |
| `extractions` | una fila por `(entidad, chunker, versión)`: estado, hashes, tamaños | Permite saber qué hay que rehacer sin volver a abrir un solo archivo. |
| **log** | `extraction_status`, `text_chars`, `text_hash`, `chunk_count`, `chunker`, `has_facet=textual` | Es **procedencia** (I9): qué extractor, cuándo, con qué resultado. Eso no se recalcula si el archivo cambia. |

La clave primaria de `extractions` y `chunks` incluye `(chunker, chunker_ver)`. Es la aplicación literal de I7: dos chunkers pueden convivir en paralelo durante una migración, que es lo que el invariante pide y lo que hace que cambiar de estrategia a los dos años no sea un evento de extinción.

El chunker aparece **además** como aserción en el log, aunque ya esté en la tabla. Redundancia deliberada: si mañana se tira el índice entero, el log sigue sabiendo con qué se fragmentó cada documento y hasta dónde llegaba la cobertura.

## Alternativas consideradas

### A. Aserciones `has_chunk` con el texto en `object_text`

La opción coherente con "todo es una aserción", y por eso la que había que descartar por escrito.

Rechazada por lo que hace la inmutabilidad al contenido derivado. Un `.md` que se edita treinta veces en un año deja treinta copias completas de su texto en un log que sólo crece, todas ellas reconstruibles desde el archivo con un `cargo run`. El log dejaría de ser un registro de lo que el sistema sabe para pasar a ser una caché con la política de retención equivocada.

Hay además un argumento de forma: una aserción es *alguien afirmó algo*. "El párrafo 14 de este PDF dice tal cosa" no es una afirmación sobre el mundo, es una copia del PDF. La procedencia, la confianza y la bitemporalidad —lo que justifica el coste de una aserción— no aportan nada aquí: la confianza siempre es 1,0 y la validez siempre es "mientras el archivo no cambie".

### B. El texto completo en una tabla y los fragmentos calculados al vuelo

Ahorra el espacio del solape entre fragmentos. Rechazada porque FTS5 necesita filas que indexar, y porque los offsets de evidencia (I9) tendrían que recalcularse en cada consulta con la versión del chunker de ese momento —justo lo que I7 quiere evitar.

### C. Un archivo o un almacén aparte para el texto

Encajaría con el almacén de blobs direccionado por hash de §7.1. Rechazada para M2: son dos cosas que sincronizar y respaldar, y ADR-0002 eligió un archivo por una razón. Si el índice llega a decenas de GB y estorba a la copia de seguridad, sacarlo es fácil precisamente porque es una proyección.

### D. No guardar el texto, sólo el índice FTS5

FTS5 sin `content=` guarda el texto igualmente, así que no se ahorra nada, y se pierde poder leer un fragmento sin pasar por el índice —que es lo que necesita `--sample` para poder auditar, y lo que necesitará M4 para embeber.

## Consecuencias

**A favor**

- **I1 se cumple de verdad y no de boquilla.** Un `DELETE FROM chunks` seguido de reextraer deja el sistema idéntico y el log intacto. Hay un test que lo comprueba.
- **Reextraer sigue a la realidad.** Editar un archivo sustituye sus fragmentos en vez de acumular versiones. Un índice que conserva el pasado no es un índice: es un log mal hecho, y ya hay uno bien hecho al lado.
- **El log queda proporcionado a la información real.** M2 añade 2.620 aserciones sobre 1.292 archivos, no una por fragmento (serían 5.015) ni una por reextracción.
- **I7 queda cableado antes de que exista un solo vector**, que es la única forma barata de tenerlo.
- El índice léxico es sustituible sin migrar datos: cambiar FTS5 por otra cosa es un `rebuild`.

**En contra**

- **Hay dos sitios donde mirar.** Una consulta que quiera texto y procedencia toca el log y la proyección. Aceptable: es exactamente el reparto que ADR-0001 y ADR-0002 ya establecieron para el grafo.
- **La proyección puede quedarse desincronizada del log** si algo escribe en una y no en la otra. Mitigado porque las dos escrituras van en la misma transacción, pero no está impuesto por el motor como sí lo están I1 e I4. Es una debilidad real y conviene decirla.
- **El texto extraído no sobrevive al archivo.** Si el archivo desaparece y el índice se reconstruye, el contenido se pierde. Conecta con la pregunta abierta de §10 del maestro ("¿el conocimiento sobrevive al archivo que lo originó?"). Si la respuesta acaba siendo que sí, hará falta un almacén de contenido de verdad, y esta decisión habrá que revisarla.
- **El solape entre fragmentos duplica texto.** Con `paragraph-pack@1` son unos 200 bytes por límite; sobre el corpus real, del orden del 10 %. Es el precio de que una frase partida siga siendo encontrable.
