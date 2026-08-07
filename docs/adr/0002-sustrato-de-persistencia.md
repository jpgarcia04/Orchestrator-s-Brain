# ADR-0002 — SQLite como sustrato único de persistencia

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0001](0001-log-de-aserciones-como-fuente-de-verdad.md)

## Contexto

El sistema necesita, en la máquina del usuario y sin proceso servidor que instalar: log append-only, travesía de grafo, búsqueda léxica, búsqueda vectorial, blobs, cifrado en reposo y capacidad de sincronizar. Restricción fundacional F1 (local-first) y F4 (escritorio Windows).

## Decisión

**SQLite como sustrato único.** El log de aserciones es una tabla append-only; grafo, FTS y vectores son proyecciones dentro del mismo archivo. Los blobs van a un almacén en disco direccionado por hash. Cifrado en reposo con SQLCipher. El índice vectorial (`sqlite-vec`) es un componente explícitamente reemplazable.

## Alternativas consideradas

### A. Pila especializada: Neo4j + Elasticsearch + BD vectorial

La respuesta de manual, y la equivocada aquí. Tres procesos servidor que el usuario tendría que instalar y mantener, tres modelos de consistencia, tres estrategias de copia de seguridad, tres superficies de sincronización, y del orden de 2 GB de RAM en reposo antes de indexar un solo archivo. Es una arquitectura de servidor trasplantada a un portátil.

### B. Motor de grafo embebido (KùzuDB, Oxigraph)

La alternativa técnicamente más interesante. Kùzu es embebido, columnar, rápido y habla Cypher.

Rechazada **como fuente de verdad**, no como opción futura. Motivos: es joven para una apuesta de una década; no cubre bien FTS ni vectores, con lo que seguiríamos necesitando SQLite al lado; y añade un segundo archivo que cifrar, sincronizar y respaldar de forma coherente con el primero.

Queda explícitamente abierta como **proyección derivada** si la travesía se convierte en cuello de botella (§7.1 del maestro). Como es reconstruible desde el log, añadirlo después es una caché, no una migración.

### C. DuckDB

Superior en analítica y agregación. Inferior en escrituras pequeñas y frecuentes y en concurrencia lectura/escritura, que es exactamente el perfil de la ingestión continua. Descartada como primaria; sigue siendo buena candidata para análisis offline sobre el log.

### D. LanceDB

Excelente para vectores y almacenamiento columnar, pero no es un motor de grafo ni relacional. Resolvería una de seis necesidades.

## Consecuencias

**A favor**

- **Un archivo.** Copia de seguridad atómica, cifrado único, sincronización de una sola cosa.
- **Cero instalación.** No hay servicio que arrancar ni puerto que abrir. Para un producto de escritorio es decisivo.
- Fiabilidad demostrada durante décadas y el motor más testeado del mundo.
- FTS5 da BM25 de calidad sin dependencia externa.
- Combinado con ADR-0001, **cualquier índice se puede sustituir sin migración de datos**.

**En contra**

- **La travesía de grafo vía CTEs recursivas es más lenta** que un motor nativo. Aceptable porque sólo hacemos travesías de profundidad acotada con presupuesto de nodos (invariante I2), nunca análisis global. Si esto deja de ser cierto, la decisión hay que revisarla.
- **`sqlite-vec` es joven** comparado con SQLite. Mitigado por ser una proyección reconstruible: si falla, se sustituye y se reindexa.
- **Un único escritor.** La ingestión concurrente necesita serializar escrituras mediante un actor dedicado. Es una restricción de diseño real, no un detalle.
- El tamaño del archivo crecerá a decenas de GB. Hay que verificar pronto el comportamiento de SQLCipher y del `VACUUM` a esa escala.
