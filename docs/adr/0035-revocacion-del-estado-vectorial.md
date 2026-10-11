# ADR-0035 — Revocación del estado vectorial al cambiar sensibilidad

- Estado: implementada; validación registrada en `../PROGRESO-CONFIANZA.md`.
- Fecha: 2026-10-11.
- Completa: ADR-0006 y ADR-0034, tras la revisión independiente de la PR #1.

## Problema confirmado

En `873a7f9`, `classify_all` podía convertir una entidad vectorizada a `vault`
sin borrar sus embeddings. `embed_all` hacía la limpieza, pero el pipeline no
lo ejecuta sin modelo. kNN filtraba el documento: eso evitaba usar el vector,
no lo eliminaba. Las purgas SQL de `Brain::correct`/`mark_folder` tampoco
retractaban `embedded_with` y `vector_count`.

Regresiones ejecutadas antes del arreglo: clasificación conservó un vector;
corrección de sensibilidad de una parte purgó parte/paquete y dejó
`embedded_with` vigente. Se usaron sólo bases sintéticas en memoria.

## Decisión

`Store::set_sensitivity(id, level, at)` es el punto común de escritura del
control. El tiempo es explícito, el mismo de la operación y sus aserciones;
no se introduce otro reloj. Entrar, permanecer o salir de `vault` elimina
todos los modelos de `vectors` para la entidad y retracta la procedencia
de embeddings aún no retractada, conocida hasta `at`. Todo comparte savepoint.

La tabla vectorial es opcional: core verifica su existencia y no crea el
esquema de M4 ni necesita un embebedor. Este acoplamiento pequeño del Store
con una proyección de seguridad es deliberado. Mantener la revocación en M4
requeriría que todas las superficies lo invocasen y falló precisamente sin
modelo. No se añaden hooks universales ni una dependencia circular de crates.

Reproyectar parte y paquetes comparte savepoint. Clasificación y marcas de
archivo/carpeta agrupan aserciones, proyecciones y revocaciones. La clasificación
escribe primero todas las señales y después proyecta archivos y sus paquetes,
de modo que incluso un paquete procesado primero ve las señales actuales de
sus partes. Las reglas actuales de herencia directa no se amplían.

`purge_vault_embeddings(at)` usa la misma revocación para reconciliar residuos,
incluida procedencia sin filas vectoriales. Se llama al clasificar sin modelo
y defensivamente desde `embed_all`. No reescribe aserciones ni fechas de
retractación anteriores; el historial permanece consultable. El filtro kNN
se conserva como defensa adicional frente a residuos o SQL interno que omita
el setter. No se promete el invariante para escrituras SQL arbitrarias.

## Retorno a un nivel menos restrictivo

Salir de `vault` limpia también posibles residuos antiguos: bajar el nivel
no reactiva embeddings revocados. Los chunks se conservan; `pending` permite
recalcular con un modelo disponible y escribir aserciones nuevas. Mientras
no haya modelo, no se inventa una procedencia vigente. Pasar entre `normal`
y `sensitive` conserva vectores locales legítimos: ambos niveles los permiten.
Una excepción posterior del usuario y quitar una marca de carpeta siguen
funcionando con la precedencia vigente.

## Límites

No hay cambio de esquema, triggers del historial ni criptografía. No se ejecuta
una migración de la bóveda personal. El conteo `entities_purged` de M4 ahora
mide residuos retirados en esa pasada; normalmente será cero porque se
revocaron al cambiar sensibilidad. Esta eliminación es lógica en SQLCipher,
no una garantía de borrado forense de páginas/WAL/backups: retención histórica
y recuperación siguen en `../RETENCION-METADATOS.md`.

Identidad entre bibliotecas y publicación segura durante indexación quedan
fuera de esta entrega y pendientes en su plan independiente.
