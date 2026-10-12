# P1: publicación segura durante la indexación

Estado: implementación de cuarentena temporal en la rama
`codex/publicacion-segura-indexacion`, creada desde `main` en `683e63b`.
No se ha abierto ni migrado ninguna bóveda personal y no se ha hecho merge.

## Alcance y reproducción

La reproducción usó exclusivamente archivos inventados bajo `%TEMP%` y bases
temporales. Se ejecutaron las herramientas de etapa sobre una base y la lectura
de `obrain-api` sobre otra conexión a esa misma base. El texto de prueba fue
`Clave ficticia: sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ12345678901234567890. Fin.`.

1. `obrain-scan` ingresó `notas.txt`; `obrain-extract` confirmó su fragmento.
2. **Antes** de `obrain-guard`, `obrain-api <db> <token>` devolvió un resultado
   BM25 con el token en el fragmento visible y `entities.sensitivity = normal`.
3. `obrain-guard` detectó `contenido:clave-de-modelo` y lo elevó a `vault`.
4. En otra base temporal, se publicó primero un documento normal, se cambió su
   contenido por el token ficticio, y se repitieron `scan` y `extract`. Antes de
   `guard`, la búsqueda volvió a devolver el texto nuevo. Después, `guard` lo
   elevó a `vault`.
5. Una imagen PNG inventada, tras `scan` y antes de `extract` o `guard`, salió
   de `obrain-api --ver <id> --json` con `kind=image`, `image` presente y
   `sensitivity=normal`. La ruta de miniatura abre el archivo del disco.
6. Un `.docx` sintético inválido produjo `1 fallos` y `1 archivos abiertos` en
   `obrain-extract`. La segunda ejecución, sin cambiar el archivo, produjo
   `0 archivos abiertos`: el fallo quedó tratado como contenido sin cambios.

La separación por programas reproduce el mismo límite de confirmación del
pipeline integrado: `indexar::index_with` confirma la ingesta, la extracción,
las relaciones de contenido y la clasificación en transacciones sucesivas.
`Store::open` usa WAL para que otras conexiones lean mientras el indexador
escribe. La prueba muestra una lectura real *entre* dos confirmaciones; no
depende de que la interfaz mantenga abierto un panel.

## Hechos del código y riesgos

**Confirmado por la reproducción:** búsqueda léxica, su fragmento, la
previsualización textual y la miniatura pueden presentar contenido antes de
clasificarlo. El caso de texto nuevo y el de texto modificado son reales.

**Deducido del código:** `extract_all_with` reemplaza `chunks` y sus triggers
actualizan FTS dentro de su transacción. La confirmación ocurre antes de
`classify_all`, que lee esos fragmentos y actualiza la columna de sensibilidad
en otra transacción. Si la clasificación falla o el proceso termina en ese
intervalo, el estado publicado persiste. La búsqueda semántica filtra `vault`,
pero una ejecución independiente de embeddings antes de la puerta vería la
sensibilidad anterior. Las rutas por ID y las listas derivadas de relaciones o
vencimientos también pueden presentar datos o evidencia del documento durante
el intervalo.

**Confirmado en la prueba de fallo y explicado por el código:** `unchanged`
compara hash y versión, pero no excluye `failed`; un error transitorio con el
mismo hash puede impedir el reintento. Además, `write_outcome` elimina los
fragmentos anteriores y registra el hash observado ante `Status::Failed`. La
previsualización de imagen lee el archivo de disco, por lo que una modificación
entre el escaneo y la apertura exige comprobar que esos bytes corresponden a
una versión publicada. `verify_candidate` valida antes de ingestar, pero la
extracción no vuelve a comprobar el hash de los bytes leídos.

**Protecciones que ya existen:** las transacciones SQLite evitan lecturas de
fragmentos *a mitad* de la sustitución; la fuga aparece *entre* etapas. La
PR #3 oculta identidades pendientes en las rutas principales de presentación
y conserva sus aserciones. El PIN retiene el contenido de entidades ya
clasificadas como sensibles, y `set_sensitivity` revoca embeddings al entrar en
`vault`. `resolve_sensitivity` conserva la precedencia de las correcciones del
usuario y la herencia de carpetas y documentos contenidos. Ninguna de esas
protecciones decide la visibilidad de una versión aún sin clasificar.

## Soluciones consideradas

### A. Cuarentena persistente por entidad (elegida para este P1)

Una proyección de publicación guarda, por entidad, la versión de archivo
observada y si su contenido está publicable. La ingesta deja la entidad en
preparación, dentro de su propia transacción, si es nueva o cambian los bytes
o su identidad. El texto extraído puede seguir en `chunks`, pero toda lectura
de contenido —FTS/BM25, kNN y generación de embeddings, previsualización,
miniatura, apertura externa, fichas y evidencia derivada— comprueba el estado
de publicación en el mismo límite de lectura. La clasificación, la sensibilidad
efectiva, la revocación y el cambio a publicable se confirman juntos, después
de verificar que extracción y hash corresponden a la observación vigente.

Un fallo deja la nueva versión en preparación. Se conservan los fragmentos
anteriores mientras no haya sustitución exitosa; durante la actualización
quedan temporalmente fuera de las lecturas del producto. No se borran
correcciones, relaciones ni aserciones históricas. La siguiente indexación
reintenta los fallos. Un escaneo sin cambios no altera el estado.

Hace falta decidir cómo inicializar bases existentes. La opción segura es
considerar no publicable el contenido sin registro de publicación hasta
reverificarlo y reclasificarlo; al actualizar la aplicación, el contenido de
esas bases quedaría temporalmente fuera de consulta. Declararlo publicable por
defecto reduciría la interrupción, pero aceptaría estados antiguos de los que
no se puede probar la correspondencia entre hash, extracción y clasificación.

Esta opción mantiene el coste de las transacciones de extracción actuales,
que ya incluyen lectura de archivos y OCR; no lo aumenta. Sacar esas operaciones
de la transacción sería una mejora adicional y una modificación mayor.

### B. Generaciones de contenido con intercambio atómico

Extraer y clasificar una generación candidata separada; después, intercambiar
en una transacción corta el puntero publicado, la sensibilidad y los índices.
Mientras tanto, la versión anterior sigue consultable con *su* clasificación.
Evita la indisponibilidad temporal y permite sacar OCR de las transacciones,
pero exige versionar o duplicar chunks/FTS, vectores y datos derivados, y
adaptar cada lectura a la generación publicada. La migración y la recuperación
de generaciones incompletas tienen más riesgo.

## Decisión e implementación

Se eligió **A**, con indisponibilidad temporal sólo de cada documento nuevo o
cuya ruta, hash o clasificación por nombre/carpeta cambió. La ingesta escribe
`content_publication.pending` en la misma transacción que la nueva observación.
La extracción conserva una copia de los fragmentos anteriores hasta publicar;
un error de lectura no los elimina y se reintenta aunque el hash observado no
haya cambiado. La extracción verifica los bytes leídos contra el hash del
escáner. Si el formato deja de ser extraíble, retira sus fragmentos activos y
conserva la copia hasta que termina la clasificación.

Las relaciones de contenido, la clasificación efectiva, la revocación, la
reconciliación de `expires_on` y el cambio a `ready` comparten una transacción.
Sólo se publica si el resultado de extracción corresponde al hash y al formato
observados; `failed` sigue oculto. El detector de fechas lee en esa transacción
sólo los fragmentos de las entidades que van a publicarse. La pasada global
posterior mantiene las inferencias de otros documentos ya publicados y omite
los pendientes. No se añadió lectura de archivos, OCR ni un recorrido completo
de fechas a la transacción de publicación. Un fallo al retractar o afirmar la
fecha revierte sensibilidad y publicación; el reintento parte de `pending`.
Las correcciones manuales no se retractan y las inferencias antiguas quedan en
el historial con `retracted_at`.
En la revisión de otras derivaciones de contenido, `dentro::link` y la
clasificación ya se confirman con la publicación; los embeddings se calculan
después y excluyen pendientes. `expires_on` era la derivación confirmada en un
commit posterior y es la que se incorporó a la publicación en esta revisión.
FTS, kNN y generación de vectores excluyen pendientes en la consulta. Las
rutas de presentación que usaban el filtro de identidad PR3 incluyen también
la cuarentena. Búsqueda, ficha y previsualización leen una instantánea WAL
coherente. La miniatura compara los bytes leídos con el hash publicado. La
apertura externa comprueba ese hash antes de entregar la ruta. El informe de
actualización y la interfaz indican cuántos documentos siguen protegidos y
que se reintentarán al actualizar.

### Migración y recuperación

La versión 2 del esquema añade tablas; no sustituye ni borra aserciones,
correcciones ni índices previos. Al abrir una base anterior, cada ruta viva
sin registro queda `pending` y se conserva su hash observado. Antes de fijar
`user_version=2`, la migración ejecuta `quick_check` y
`foreign_key_check`. La inserción y el cambio de versión comparten un
savepoint; una apertura posterior repara también filas que hubieran faltado
por una interrupción. El arranque reproyecta relaciones, sensibilidad y
vencimientos en una transacción. Sólo libera texto heredado si su extracción
verificable coincide con la observación; para formatos sin extractor disponible
exige que no queden fragmentos activos ni un fallo previo. Una extracción
ausente o fallida permanece protegida hasta una indexación posterior. Ni la
migración ni el arranque leen los archivos de la biblioteca.

`Brain::open`, el constructor fallible `Brain::try_from_store` y la
inicialización explícita que emplean CLI y el indexador de Tauri comparten
`Brain::initialize_store`. Así, una base v1 no entrega listas vacías por haber
omitido el bootstrap: el contenido verificable se recupera antes de leerlo y
un fallo devuelve un error que permite reintentar. `Brain::from_store` usa
también esa inicialización y, al conservar su firma infalible por
compatibilidad, produce un error de pánico explícito si no puede completarla;
las entradas de producción usan la variante fallible o comprueban la
inicialización antes de construirlo.
Los documentos sin extracción verificable permanecen en cuarentena; sus
aserciones e índices se conservan para la recuperación posterior.

El respaldo de `publication_backup_chunks` conserva la proyección textual
anterior de cada documento pendiente; se elimina tras una publicación válida.
La copia de la base completa previa a desplegar sigue siendo la vía de
recuperación ante un defecto desconocido de la nueva versión. Esta rama sólo
ha ejercitado la migración con bases sintéticas y una copia aislada; ningún
comando de migración se ha ejecutado sobre la bóveda personal.

### Comprobaciones y límite de la ruta externa

Las regresiones nuevas fallaron antes de la corrección por un resultado BM25
visible entre extracción y clasificación. Ahora cubren documentos nuevos y
modificados, fallos de extracción y clasificación con reintento, conservación
de la corrección y los fragmentos anteriores, aislamiento de los demás
documentos, miniaturas,
apertura, idempotencia, renombre a formato sin extractor, migración en copia e
instantáneas concurrentes. La suite existente comprueba PIN, herencia e
identidad pendiente.

La revisión independiente de la PR #4 encontró además una fecha inferida de
la versión anterior visible después de publicar texto nuevo. Una regresión
provocó un fallo real en la retractación mediante un trigger SQLite sintético:
antes del arreglo `index_with` dejaba el documento en `ready`; después del
arreglo el commit se revierte y el documento sigue pendiente. Otras regresiones
comprueban la lectura WAL concurrente, la idempotencia y la conservación de una
fecha manual. La recuperación se prueba también con bases construidas desde el
DDL v1 de `683e63b`, con proyecciones antiguas que representan un corte entre
extracción y detección de vencimientos. Cubren API, CLI, fallo y reintento del
bootstrap (incluido el error explícito de la CLI), extracción verificable e
inverificable, corrección manual, log e índice textual. No se derivan sólo de
borrar tablas de una base v2.

La apertura con una aplicación del sistema recibe una ruta mutable. El núcleo
comprueba su hash inmediatamente antes de devolverla, pero otro proceso puede
cambiar el archivo entre esa comprobación y la apertura por el sistema
operativo. Esa carrera externa no afecta al texto ni a las miniaturas que
sirve el núcleo. No se debe ejecutar simultáneamente un indexador de una
versión anterior durante el despliegue: ese binario desconoce la cuarentena.
Si una versión publicada no tiene hash verificable, no se entrega su ruta para
abrirla ni se genera miniatura: se puede reintentar tras una observación
completa del archivo.

En el entorno de pruebas Windows, dos migraciones de bases v1 sintéticas
distintas ejecutadas por hilos sucesivos del mismo proceso de pruebas
provocaron un `STATUS_ACCESS_VIOLATION` durante la apertura. Dos aperturas en
un mismo hilo y la reapertura de una única base cifrada desde otro hilo
pasaron. Por ello cada escenario v1 de integración corre en su propio
ejecutable. La causa nativa y la migración de varias bases distintas dentro
de un solo proceso quedan sin verificar; la CLI y Tauri usan una base por
proceso en el flujo de esta PR.

### Validación en esta rama

- La reproducción inicial hizo fallar las pruebas de búsqueda antes del cambio.
- `cargo test --locked --workspace` pasó 57 grupos de pruebas, incluidos los
  nuevos escenarios de revisión y las bases v1 sintéticas.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` pasó.
- `cargo check --locked --manifest-path ui/src-tauri/Cargo.toml` pasó.
- `npm run check` pasó con cero errores y advertencias; `npm test` pasó 12 pruebas.
- La migración se ejecutó sólo sobre una copia de base sintética, con
  comprobaciones del log, texto, corrección de usuario, integridad, reparación
  de un backfill interrumpido y reapertura idempotente.
