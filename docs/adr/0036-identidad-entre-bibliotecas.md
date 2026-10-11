# ADR-0036 — Inventario global antes de reconciliar documentos

- Estado: implementada en `codex/identidad-entre-bibliotecas`; pendiente de revisión de PR.
- Fecha: 2026-10-11.

## Problema

El reconciliador de M1 comparaba el `FileID` anterior sólo con los candidatos
de la biblioteca que estaba escribiendo. Si un archivo salía de A hacia B y
otro ocupaba su ruta, indexar A antes de B reutilizaba su `EntityId`. Al invertir
las bibliotecas se reproducía el mismo fallo con origen B. El reemplazo heredaba
correcciones, sensibilidad, colecciones y relaciones del archivo movido.

## Decisión

`indexar` reúne todas las bibliotecas registradas y las nuevas, incluso cuando
la llamada pide sólo una. Escanea y hashea fuera de la transacción; deduplica
las rutas de raíces solapadas y vuelve a verificar metadatos y `FileID` antes
de escribir. Sólo entonces ingesta el lote global en una transacción. Un
`FileID` anterior presente en cualquier biblioteca impide reconciliar por ruta;
el movimiento conserva su entidad y el reemplazo obtiene otra.

Una biblioteca ausente, un recorrido parcial o un error de lectura aplaza la
pasada completa, sin escrituras. Las ausencias se comprueban fuera de la
transacción y se aplican con la ingesta; una entidad vista en otra biblioteca
no entra en papelera. Los guardados atómicos siguen reconciliándose por ruta
cuando el inventario es completo y la identidad anterior ya no aparece.

Un candidato sin `FileID`, con un `FileID` duplicado o con propietarios
incompatibles se aplaza. Se retracta su ruta activa y se guarda
`identity_pending_path` en el log para recuperarla cuando un recorrido posterior
aporte evidencia suficiente. Sin ruta activa no pasa a los cargadores de
documentos ni a la búsqueda. La presentación consulta además la marca pendiente:
el árbol de carpetas y sus derivados (cielo, explorador, recientes, sensibles,
vencimientos y colecciones) no cuentan ni enumeran esa entidad aunque conserve
`contained_by`. La lente, los paquetes y la ficha de otras entidades omiten sus
referencias mientras esté pendiente. Las lecturas directas de ficha y
previsualización por `EntityId` la rechazan antes de consultar chunks; apertura
y revelado no obtienen ruta. Se comprueba la marca positiva, no la mera ausencia
de ruta: una entidad de la papelera mantiene su vista histórica y su política
de retención propia.

La interfaz retira la selección y desmonta las vistas documentales al comenzar
la indexación. Las respuestas iniciadas antes de ese cambio se descartan y,
al terminar, se vuelve a leer la carpeta o colección abierta. Una vista previa
anterior no reaparece de forma implícita; la persona puede abrirla de nuevo
cuando la identidad esté resuelta. Si durante ese recorrido falta otra ruta previa,
también se retiene como pendiente: una pasada posterior podrá recuperarla o
enviarla a papelera cuando el inventario sea fiable. Se conservan la entidad,
sus decisiones, relaciones, sensibilidad, procedencia, chunks y aserciones
históricas. Al resolverse la marca, el mismo `EntityId` vuelve a las vistas con
sus datos legítimos; un reemplazo de ruta no hereda esos datos. El informe y la
app muestran que hay documentos pendientes; actualizar vuelve a intentarlo. El CLI de escaneo independiente
rechaza escrituras parciales o sobre una base con otras bibliotecas.

## Límites

La comprobación del sistema de archivos es optimista: no existe una operación
atómica compartida entre NTFS y SQLite. Un cambio posterior a la última
verificación requiere otra pasada. La transacción de M1 sólo escribe base de
datos; no incluye lectura de archivos, hashing ni OCR. Las etapas posteriores
del pipeline conservan su arquitectura previa; el P1 de publicación segura
durante indexación sigue pendiente.

Las pruebas usan archivos sintéticos y bases en memoria. Ninguna migración se
ha ejecutado sobre la bóveda personal.

La retirada de la interfaz empieza cuando recibe el evento de comienzo de
indexación; no crea una transacción atómica entre NTFS, todas las etapas del
pipeline y una ventana que aún no haya recibido ese evento. La publicación
segura durante la indexación sigue siendo un P1 independiente.
