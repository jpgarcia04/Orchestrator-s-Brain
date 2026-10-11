# Investigación P1: publicación durante la indexación

Estado: propuesta de diseño; no se ha cambiado el esquema ni el pipeline. Rama
`codex/publicacion-segura-indexacion`, creada desde `main` en `683e63b`.

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

### A. Cuarentena persistente por entidad (preferida para este P1)

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

## Decisión necesaria antes de implementar

Recomiendo **A** con cuarentena temporal también de la versión anterior en las
lecturas del producto. Es la reparación más acotada para el P1. Aun así afecta
el esquema de bases cifradas existentes y las rutas de lectura de varios
crates. La decisión de inicialización de bases existentes y la indisponibilidad
temporal son cambios visibles que conviene aprobar antes de escribir la
migración. B queda como alternativa si mantener accesible la versión previa
durante la actualización es un requisito estricto.

Tras elegir, las primeras pruebas deben demostrar el fallo actual en
`Brain::search`, `Brain::preview`, lectura concurrente y miniatura; después
añadir fallo y reintento, identidad pendiente, PIN, herencia, historial,
reindexación sin cambios y las rutas semánticas. La garantía es sobre las
lecturas del producto: un programa que abre directamente un archivo mutable
en el sistema operativo tiene una carrera distinta que debe tratarse en la
autorización de apertura.
