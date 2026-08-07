# ADR-0003 — Rust para el núcleo de indexación

- **Estado:** Aceptada
- **Fecha:** 2026-08-07

## Contexto

El núcleo es un demonio que corre **todo el día** en la máquina del usuario y que debe: recorrer y hashear cientos de miles de archivos, extraer texto de formatos pesados, ejecutar inferencia local, mantener índices bajo mutación concurrente y no molestar. Restricciones F1 (local-first) y F4 (escritorio Windows).

La petición explícita fue argumentar la elección, incluyendo por qué no Java.

## Decisión

**Rust** para el núcleo (indexación, almacenamiento, búsqueda, routing de IA), expuesto como binario nativo único. **TypeScript** para la UI sobre Tauri v2.

## Alternativas consideradas

### A. Java / JVM

Rechazada. Las fortalezas reales de Java —ecosistema servidor maduro, mantenibilidad en equipos grandes, throughput del JIT en procesos de larga vida— no aplican a un demonio local monousuario. Los costes sí aplican todos: línea base de memoria alta para un proceso permanentemente residente, pausas de GC durante escaneos intensivos que se traducen en una UI que tartamudea, y la necesidad de empaquetar un runtime en una app de escritorio (jlink lo mitiga, no lo elimina).

Java sería una elección **defendible para el relay de sincronización** en la nube, si algún día existe. Es la equivocada para el núcleo.

### B. Node / TypeScript en todo el stack

Atractivo por tener un solo lenguaje. Rechazado: el bucle de eventos monohilo es lo contrario de lo que pide una carga CPU-bound de hashing y extracción. `worker_threads` es un parche sobre un modelo que no fue diseñado para esto. Y el consumo en reposo vuelve a ser el problema. Node encaja como pegamento; no como motor.

### C. Go

**La alternativa honestamente defendible, y conviene dejarlo escrito.** Más simple que Rust, curva de aprendizaje mucho menor, concurrencia excelente, binario único, compilación rápida. En igualdad de condiciones sería la elección pragmática.

Se elige Rust por tres razones concretas:

1. **Pausas de GC.** Go las tiene, cortas pero reales. Durante un escaneo de 300.000 archivos con la UI viva, la latencia predecible importa más que la latencia media.
2. **Ecosistema de ML local.** `candle` y `ort` en Rust son bastante superiores a lo disponible en Go para inferencia local, y la inferencia local es fundacional aquí (F1), no accesoria.
3. **Mutación concurrente del índice.** Es constante en este sistema. Las garantías de propiedad de Rust convierten en errores de compilación una clase de bugs que en Go serían carreras detectables sólo en producción.

Si en seis meses la productividad en Rust resulta ser el cuello de botella real del proyecto, esta decisión merece un ADR que la supersede. No es una decisión irreversible.

### D. C#/.NET

Buena integración con Windows y buen rendimiento. Rechazada por dos motivos: ataría la arquitectura al ecosistema Microsoft justo cuando F4 dice "Windows *primero*", no "Windows siempre"; y arrastra los mismos costes de GC y runtime que Java, aunque más suaves.

## Consecuencias

**A favor**

- Memoria predecible y baja para un proceso permanentemente residente.
- Sin pausas de GC durante escaneos grandes.
- Binario nativo único: no hay runtime que instalar.
- Ecosistema adecuado y maduro: `rusqlite`, `tantivy`, `candle`/`ort`, `notify`, `blake3`, `tokio`.
- Paralelismo real y seguro (`rayon`) para hashing y extracción.

**En contra**

- **Curva de aprendizaje pronunciada.** Es el coste más alto de esta decisión y es real, no teórico.
- **Iteración más lenta**, especialmente al principio y en tiempos de compilación.
- La frontera Rust↔TypeScript en Tauri añade fricción: hay que definir y mantener el contrato IPC con cuidado.
- Menos candidatos si el proyecto llega a necesitar equipo.
