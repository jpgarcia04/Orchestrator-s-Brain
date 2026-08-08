# ADR-0011 — El rerank se difiere, con la medición que lo justifica

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0008](0008-modelo-de-embeddings-y-runtime.md)

## Contexto

§6 del maestro describe cinco pasos de recuperación. El cuarto es:

> *Rerank — cross-encoder local sobre el top-50.*

Un cross-encoder es cualitativamente distinto de un bi-encoder como E5. E5 embebe la consulta y el documento **por separado** y compara vectores, así que los 2.479 vectores del corpus se calculan una vez y se reutilizan siempre. Un cross-encoder lee **el par** consulta-documento junto, en la misma pasada, y por eso entiende la relación entre los dos — y por eso **no se puede precalcular nada**: son 50 pasadas completas del modelo en cada consulta.

El candidato de referencia, `bge-reranker-v2-m3`, tiene 568 M de parámetros.

## Decisión

**El rerank se difiere. La forma del pipeline lo admite —es un paso entre la fusión y la respuesta— y el hito se cierra sin él.**

### La medición

No se ha medido `bge-reranker-v2-m3`: son ~2,2 GB que no se han descargado. Lo que sí se puede medir sin bajar nada es el **suelo**: 50 secuencias largas (~400 tokens, cerca del tope de 512) por el modelo que ya está cargado, que tiene 118 M de parámetros. Un cross-encoder de 568 M sobre el mismo top-50 no puede ser más rápido que eso.

| | |
|---|---|
| 50 secuencias largas por `e5-small` (118 M) | **4,5 s** ← medido |
| Extrapolación a 568 M por relación de parámetros (×4,8) | **~21 s** ← extrapolado, **no medido** |
| Consulta híbrida completa hoy (BM25 + kNN + RRF) | **43 ms** |
| Listón de M2 para «instantánea» | 0,5–1,9 ms |

Cuatro órdenes de magnitud por encima del listón, y tres por encima de lo que cuesta hoy la consulta entera. No es una diferencia que se afine: es una diferencia que decide.

La extrapolación por número de parámetros es tosca —ignora la profundidad, la dimensión oculta y el tamaño de la tabla de embeddings— pero el suelo medido, 4,5 s, **ya basta por sí solo**. Aunque el reranker fuera milagrosamente igual de rápido que un modelo cinco veces más pequeño, 4,5 s siguen sin caber.

### Lo que se pierde por diferirlo, con un ejemplo

El rerank no es un adorno, y este corpus enseña exactamente para qué sirve. Consulta `universidad`:

```
BM25 →  1. obrain-extract/src/format/ooxml.rs      (contiene la cadena "Universidad" en un test)
        2. docs/adr/0011-rerank-diferido.md        (este mismo documento)
kNN  →  1. Escritorio\MatriculaJuanPabloGarcia.pdf (NO contiene la palabra)
RRF  →  1. docs/PROJECT-MASTER.md   [bm25#3 knn#4]
        …  MatriculaJuanPabloGarcia.pdf fuera del podio
```

La fusión pone documentación y código por delante del documento correcto, porque BM25 está muy seguro de sus coincidencias literales y varias aparecen también en la rama vectorial. Un cross-encoder sobre esos candidatos vería que un fixture de test que menciona «Universidad Canadá» no responde a la pregunta y que un formulario de matrícula sí. **Es justo el trabajo que falta**, y por eso se difiere en vez de declararlo innecesario.

## Alternativas consideradas

### A. Incluirlo igualmente y aceptar los ~21 s

Rechazada: convierte una búsqueda en un trabajo por lotes. La cuña del producto (F3) es «encuentra lo que no recuerdas que tienes», y eso se usa escribiendo y mirando.

### B. Un cross-encoder mucho más pequeño

`ms-marco-MiniLM-L6-v2` son 22 M de parámetros: por la misma regla de tres, unos 0,9 s sobre el top-50 — todavía veinte veces el listón, y además es monolingüe inglés, que es la primera restricción que ADR-0008 usó para descartar modelos. `bge-reranker-base` (278 M) tampoco cabe. **No hay un cross-encoder multilingüe que quepa en el presupuesto de esta CPU.** Diferir no es pereza: es que la opción no existe hoy.

### C. Rerank sobre el top-5 en vez del top-50

Diez veces menos: ~2,3 s. Rechazada por dos motivos. Sigue sin caber, y sobre todo destruye el propósito: reordenar cinco candidatos no puede rescatar el documento que está en el puesto 12. El valor del rerank está en mirar hondo.

### D. Rerank asíncrono: resultados inmediatos que se reordenan solos

La opción que probablemente acabe ganando. Se muestra la fusión al instante y el rerank llega después y reordena. Rechazada **para M4** porque exige una UI —no hay— y porque I10 dice que la animación jamás está en el camino crítico de la información: una lista que se recoloca sola bajo el cursor es exactamente eso, y diseñarla bien es trabajo de la fase de experiencia, no de este hito.

### E. Rerank en GPU

La RTX 4050 tiene 6 GB y `bge-reranker-v2-m3` cabría. Rechazada por lo mismo que ADR-0008 rechazó la GPU para embeber: es complejidad de despliegue y de soporte para una máquina concreta, y convierte un requisito de hardware opcional en obligatorio. Cuando exista un camino de GPU, se revisa — y entonces el número que hay que medir es éste.

## Consecuencias

**A favor**

- **La consulta es instantánea de verdad:** 43 ms de extremo a extremo, incluido embeber la consulta.
- **La forma del pipeline no cambia** cuando el rerank llegue: es un paso entre la fusión y la respuesta, y `search::hybrid` ya devuelve las tres listas por separado para poder compararlas.
- **Queda escrito qué se está perdiendo**, con un ejemplo del corpus real y no con una frase general.

**En contra**

- **La fusión deja ganar a la coincidencia literal** cuando BM25 está muy seguro, aunque el documento correcto lo traiga sólo la rama vectorial. Es el defecto conocido y es el que el rerank arreglaría.
- **La extrapolación no es una medición**, y aquí está etiquetada como tal. El día que se descargue el reranker hay que sustituir el número y volver a decidir.
- **El presupuesto es de esta máquina.** Un portátil más lento lo empeora; uno con más núcleos lo mejora, pero no tres órdenes de magnitud.
- **`Capability::Reranker` existe en la puerta y no tiene implementación.** La puerta ya decide sobre él —es la única capacidad que `vault` puede usar en local, porque no persiste derivados— y esa rama de la política sigue siendo código sin ejercitar contra nada real.
