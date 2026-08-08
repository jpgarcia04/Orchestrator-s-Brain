# ADR-0009 — Los vectores son una proyección y su identidad I7 está en la clave primaria

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0005](0005-donde-vive-el-texto-extraido.md), [ADR-0008](0008-modelo-de-embeddings-y-runtime.md)
- **Contesta a:** el invariante I7

## Contexto

ADR-0005 ya decidió que el texto extraído es una proyección y no una aserción. Los vectores heredan ese razonamiento entero —son una derivación de una derivación— y la pregunta de *dónde viven* se contesta sola: en su propia tabla.

Lo que no se contesta solo es **cómo se cablea I7**:

> *Todo vector guarda modelo + versión + dimensión + chunker. Dos índices en paralelo durante migraciones.*

Guardar esos cinco datos es fácil. Lo que decide si el invariante sirve para algo es **dónde** se guardan, porque §5.3 describe una migración muy concreta:

> *Crear el índice nuevo en paralelo, re-embeber en segundo plano con presupuesto, conmutar cuando la cobertura sea suficiente, retirar el viejo.*

«En paralelo» significa que en algún momento hay dos vectores del mismo fragmento a la vez. Si la clave de la tabla es el fragmento, eso es imposible: el segundo modelo pisa al primero y la migración deja de ser gradual para convertirse en un corte.

## Decisión

**Tabla `vectors` propia, con `(chunk, model_id, model_ver, chunker, chunker_ver)` como clave primaria.**

```sql
CREATE TABLE vectors (
    chunk       INTEGER NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
    entity      TEXT    NOT NULL REFERENCES entities(id),
    model_id    TEXT    NOT NULL,
    model_ver   INTEGER NOT NULL,
    dim         INTEGER NOT NULL,
    chunker     TEXT    NOT NULL,
    chunker_ver INTEGER NOT NULL,
    vec         BLOB    NOT NULL,
    PRIMARY KEY (chunk, model_id, model_ver, chunker, chunker_ver)
);
```

Cinco consecuencias que son la decisión:

1. **Dos modelos conviven.** Insertar con otro `model_id` no pisa nada. Hay un test que lo comprueba, y otro que comprueba que el kNN de cada modelo sólo ve los suyos —comparar vectores de modelos distintos daría números que parecen puntuaciones y no significan nada—.
2. **Reembeber sustituye, no acumula.** Con la misma clave, `INSERT OR REPLACE`.
3. **El chunker está aunque el `chunk_id` ya lo implique.** Redundancia deliberada, por el mismo motivo que en ADR-0005: si mañana se tira `chunks` entero, un vector tiene que poder decir por sí solo de dónde salió. Un vector huérfano que no sabe con qué se fragmentó es un vector que hay que tirar.
4. **`vec` es un BLOB de `dim` floats de 32 bits en little-endian**, no JSON: 3,6 MB frente a unos 40 en este corpus, y el kNN lee el bloque sin parsear.
5. **`ON DELETE CASCADE` en `chunk`.** No es higiene: es lo que permite que M2 siga existiendo sin saber que M4 existe. Reextraer un archivo editado **borra y reinserta** sus fragmentos, y sin el cascade esa operación choca contra la clave ajena y tumba el pipeline entero con `FOREIGN KEY constraint failed`. La alternativa —que el extractor borre los vectores antes— obligaría a M2 a conocer una tabla que se creó dos hitos después. La semántica además es la que se quiere: si el texto cambió, el vector de ese texto es basura. Este punto **no estaba en el diseño original**; lo puso ahí un fallo real, ver Consecuencias.

### Al log va la procedencia, y sólo eso

Dos aserciones por entidad vectorizada —`embedded_with` (`multilingual-e5-small@1`) y `vector_count`—, con el mismo criterio de M2: se escribe lo que no se puede recalcular mirando la proyección. Sobre el corpus real son **776 aserciones para 388 entidades y 2.479 vectores**. Ni una por vector.

El vector no va al log. Es proyección (I1) y además sería un derivado del contenido, permanente e inmutable, dentro de un registro que no se borra — exactamente el error que ADR-0006 evita al prohibir que la evidencia de sensibilidad contenga el texto encontrado.

### El origen es la puerta, no `chunks`

`embed_all` recibe las entidades de `gate::embeddable` y **además** llama a `gate::check` por entidad justo antes de invocar el modelo. Las dos comprobaciones parecen redundantes y no lo son: la primera es una consulta de conjunto que alguien podría cambiar por comodidad dentro de dos años; la segunda va pegada a la invocación. El contador `denied_at_call` mide si las dos están de acuerdo, y si algún día no lo están lo dirá un número en vez de pasar inadvertido.

## Alternativas consideradas

### A. Los cinco datos en una tabla de configuración `vector_index_meta`

Una fila que dice «este índice es de tal modelo». Rechazada porque hace imposible la migración en paralelo de §5.3 —sólo hay un índice— y porque convierte un dato que describe cada vector en un dato global: un vector suelto deja de saber de dónde salió, que es literalmente lo que I7 prohíbe.

### B. Las columnas en la tabla pero fuera de la clave primaria

Parece equivalente y no lo es. Con la clave en `chunk` a secas, insertar el vector del modelo nuevo borra el del viejo por definición de la clave. La migración pasa de gradual a instantánea, y «conmutar cuando la cobertura sea suficiente» no puede ocurrir porque nunca hay cobertura parcial de dos cosas.

### C. Una tabla por modelo (`vectors_e5_small_v1`, …)

Aísla perfectamente y hace trivial retirar un modelo (`DROP TABLE`). Rechazada por lo que le hace al resto: el nombre de la tabla pasa a ser un dato variable dentro del SQL, el censo de cobertura exige leer el catálogo de SQLite, y una consulta que quiera fusionar dos índices durante la conmutación necesita SQL generado. Se cambia una `WHERE` por metaprogramación.

### D. `sqlite-vec` como almacén, además de como índice

Ver [ADR-0010](0010-recorrido-lineal-frente-a-indice-ann.md). Además de la discusión de rendimiento, delegar el almacenamiento en una extensión significa que el esquema de I7 lo decide la extensión.

### E. Guardar los vectores en `float16` o cuantizados a int8

Divide el tamaño por 2 o por 4. Rechazada porque no hay problema que resolver: 3,6 MB. Se revisará cuando el índice pese, y entonces será una decisión con una medición de pérdida de calidad delante.

## Consecuencias

**A favor**

- **La migración de §5.3 es ejecutable, no un párrafo.** Dos modelos en la misma tabla, cobertura por modelo consultable (`index::census`), y el kNN filtrando por `(model_id, model_ver)`.
- **`DROP TABLE vectors` no pierde nada.** I1 se cumple de verdad: reconstruir es volver a ejecutar. Hay un test que además comprueba lo importante — que al reconstruir, una entidad reclasificada a bóveda **se queda fuera**.
- **La aritmética cierra y se puede comprobar.** 2.507 fragmentos − 28 retenidos por la puerta = 2.479 vectores. El CLI lo imprime y marca si no cuadra.
- **El log queda proporcionado.** 776 aserciones para 2.479 vectores.
- **El vector es autodescriptivo.** Los cinco datos viajan con él.

**En contra**

- **La primera versión de esta tabla rompía el pipeline y ningún test lo vio.** Sin `ON DELETE CASCADE`, editar un archivo ya vectorizado hacía caer `obrain-extract` con `FOREIGN KEY constraint failed`. Los tests de M4 no podían verlo porque M4 nunca borra fragmentos, y los de M2 tampoco porque no conocen la tabla `vectors`. Lo destapó reescanear el repositorio real después de editar cinco archivos. Es la consecuencia incómoda de que una proyección referencie a otra: **añadir una capa puede romper la de abajo sin tocarla**, y la única prueba que lo detecta es ejecutar el pipeline entero. Hay ahora un test de regresión con ese caso.
- **Se duplican `chunker` y `chunker_ver`**, que ya están en `chunks`. Es redundancia consciente; también significa que una fila de `vectors` puede contradecir a su `chunk` si alguien los escribe por separado, y nada en el motor lo impide.
- **La clave primaria es de cinco columnas.** Ocupa más de lo que ocuparía un entero, y SQLite mantiene el índice de la clave además de la tabla.
- **Nada obliga a que `dim` coincida con la longitud del BLOB** salvo el código que lo escribe. `index::insert` comprueba la dimensión y la norma; una escritura por otra vía no lo haría. Un `CHECK (LENGTH(vec) = dim * 4)` cerraría el hueco y no está.
- **Reclasificar a bóveda no borra los vectores existentes.** La proyección sólo se corrige al reconstruirla. Es coherente con I1 —quien mantiene una proyección es quien la reconstruye— pero significa que hay una ventana en la que un documento es `vault` y sus vectores siguen ahí. Cerrarla exige que el clasificador toque la proyección, lo que invierte la dependencia entre capas. Se deja anotado, con un test que fija el comportamiento actual.
- **No hay presupuesto ni reanudación.** `embed_all` vectoriza todo lo pendiente de una vez, dentro de una transacción. Sobre 2.479 fragmentos son 4,6 minutos; sobre 200.000 sería una transacción de horas. §4.4 pide una cola diferida con presupuesto y priorizada por atención, y eso no existe.
