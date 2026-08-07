# ADR-0001 — Log de aserciones como fuente de verdad

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Supersede a:** —

> Los ADR no se editan. Si esta decisión cambia, se escribe uno nuevo que la supersede.

## Contexto

Orchestrator's Brain tiene que sostener simultáneamente seis cosas que normalmente se implementan como features separadas y caras:

1. Recordar el pasado, no sólo el presente ("¿en qué trabajaba en marzo de 2023?").
2. Distinguir hecho observado de inferencia, y explicar el porqué.
3. Permitir que el usuario corrija al sistema, y que la corrección **persista** frente a reescaneos.
4. Detectar contradicciones a lo largo del tiempo.
5. Auditoría de todo lo que ocurre con datos sensibles.
6. Sincronizar entre dispositivos sin servidor autoritativo (arquitectura local-first).

## Decisión

La unidad primaria de datos es la **Aserción**, no el nodo. El sistema mantiene un **log append-only de aserciones inmutables**, con tiempo bitemporal, procedencia, confianza y evidencia. El grafo, el índice léxico y el índice vectorial son **proyecciones reconstruibles** de ese log.

Nada se borra: se **retracta** (`retracted_at`).

## Alternativas consideradas

### A. Grafo mutable como fuente de verdad (el modelo por defecto)

Nodos y aristas con `UPDATE` y `DELETE` directos. Es lo que hace prácticamente todo el sector.

Rechazada porque cada uno de los seis requisitos se convierte en un sistema paralelo: una tabla de historial para el punto 1, columnas `is_ai_generated`/`confidence` desperdigadas para el 2, una tabla de overrides con lógica de precedencia ad hoc para el 3, un job de detección para el 4, un log de auditoría separado para el 5, y reconciliación de estado —el problema difícil— para el 6. Seis subsistemas que hay que mantener coherentes entre sí. El coste no aparece el primer año; aparece el tercero.

### B. Event sourcing sobre entidades de dominio

Log de eventos de negocio (`DocumentoIndexado`, `RelaciónCreada`) con proyecciones.

Rechazada por un motivo sutil: los eventos de dominio codifican *qué hizo el sistema*, no *qué es cierto*. La bitemporalidad y las contradicciones necesitan lo segundo. Un evento `RelaciónCreada` no sabe expresar "esto fue cierto entre 2021 y 2023". La aserción es la granularidad correcta.

### C. Triple store / RDF puro

Muy cercano en espíritu, y honestamente la alternativa más fuerte.

Rechazada por ergonomía y ecosistema: los triple stores embebidos son escasos, SPARQL es hostil para un producto de consumo, la reificación —necesaria para colgar confianza y procedencia de un triple— es notoriamente incómoda, y perderíamos FTS y vectores en el mismo motor. Nos quedamos con el **modelo conceptual** de RDF (sujeto-predicado-objeto, vocabulario controlado) sin adoptar su pila.

## Consecuencias

**A favor**

- Auditoría, deshacer, explicabilidad, detección de contradicciones y viaje en el tiempo dejan de ser features y pasan a ser propiedades emergentes.
- Sincronizar es replicar un log append-only en vez de reconciliar estado: un orden de magnitud más simple.
- Cambiar de motor de búsqueda, de modelo de embeddings o de representación del grafo deja de ser una migración y pasa a ser un `rebuild`.
- La precedencia del usuario sobre la inferencia se implementa una sola vez, en la resolución de la proyección.

**En contra — hay que asumirlo con los ojos abiertos**

- **El log crece y no se poda.** Un escaneo grande genera millones de aserciones. Habrá que diseñar compactación de aserciones deterministas repetidas, y no es trivial.
- **Toda lectura pasa por una proyección**, con lo que la coherencia proyección↔log es un invariante que hay que testear activamente.
- **Escribir es más caro** que un `UPDATE`: cada cambio es una entrada nueva más la actualización de índices.
- **Es más difícil de razonar** para quien llegue nuevo al código. Requiere documentación viva.
