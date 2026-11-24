# Progreso — 2026-10-11

**Actualización:** la PR #2 quedó fusionada en `main` (`48f73f5`). Las
referencias más abajo a la rama previa y a la base del PR documentan el
estado de aquella entrega, no el estado actual. La identidad entre bibliotecas
se trabaja desde `main` en `codex/identidad-entre-bibliotecas` (ADR-0036).

## Puerta de Aetheris, paso 2: ninguna copia de algo protegido — 2026-11-24

La condición 1 del [ADR-0039](adr/0039-la-puerta-de-aetheris-se-adelanta.md).
`Elegibles::SoloNormal` deja fuera, dentro de la consulta y antes del
`LIMIT`, todo documento cuyo texto extraído (`text_hash` de `extractions`)
es el de una entidad que no es `normal`, aunque él lo sea y aunque lo
protegido esté en la papelera. Un texto vacío no cuenta. La ventana no
cambia: sigue usando `Ventana`.

Pruebas en las dos ramas, con bibliotecas sintéticas en memoria: la copia
sin marcar de un documento de bóveda no sale en BM25, sea la copia más vieja
o la más nueva, ni con lo protegido en la papelera; dos textos vacíos no se
toman por copias; en kNN la copia no sale aunque sea lo más cercano. Cada una
falló con una mutación que quitaba la regla o la excepción del texto vacío.
Un índice nuevo de la proyección, `idx_extractions_text_hash`, y una prueba
del plan: la primera versión de la consulta recorría todas las extracciones
por cada resultado, y la prueba lo cazó.

## Puerta de Aetheris, paso 1: abrir para leer — 2026-11-24

El primer paso del orden del [ADR-0039](adr/0039-la-puerta-de-aetheris-se-adelanta.md)
(su condición 3). `Store::abrir_para_leer` (`obrain-core/src/lectura.rs`)
y `obrain_api::lectura::abrir_para_leer` abren la base cifrada sin crear,
cifrar, migrar ni reparar nada, con `SQLITE_OPEN_READ_ONLY` y
`query_only`. Lo que la base pida además de leerla tal como está se niega con
el `no_disponible` del ADR-0038: `base_cerrada` (sin base, sin llave, en
claro o con otra llave), `ocupada` (restauración, cifrado o llave a medias,
`-journal`, un log sin su `-shm` o al revés) y `version_distinta` (otra
versión del esquema o sin las tablas de las proyecciones).

Abrir en sólo lectura no bastaba para no escribir. Medido con SQLCipher en
Linux: sin log, una apertura normal crea `-wal` y `-shm` y los deja, y
`readonly_shm` falla dejando un `-wal` vacío; con el log de un corte, la
apertura normal reescribe `-shm`. Por eso hay dos modos: con log
(`readonly_shm=1`, lee el de la ventana o el de un corte sin tocarlo) y sin
log (`immutable=1`, con la huella del archivo comprobada después de cada
lectura; si alguien escribió, lo leído se tira y la respuesta es `ocupada`).
Cada lectura pasa por `Lectura::leer`, que no deja llegar al `Store` de otra
forma.

Pruebas en `obrain-api/tests/lectura.rs`, con bases sintéticas cifradas:
sola, con otro proceso que la tiene abierta y acaba de escribir (la
«ventana», un proceso hijo), con el log de un corte, con una escritura a
mitad de la lectura, con una escritura intentada desde la lectura, cada caso
de `ocupada`, `base_cerrada` y `version_distinta` sin que cambie un byte, y
una restauración de respaldo que funciona en cuanto la lectura se suelta.
Cada protección falló su prueba con una mutación temporal que la quitaba.

`cargo test --workspace` en Linux pasa entero salvo dos pruebas de
`respaldos.rs` que ya fallan en `main` en Linux
(`restauracion_rechaza_conexiones_abiertas` y
`restauracion_cancelada_por_base_en_uso_no_revierte_trabajo_posterior`): una
conexión del mismo proceso no bloquea la restauración porque los bloqueos
POSIX son por proceso. Queda por comprobar en Windows que `readonly_shm` e
`immutable` se portan igual y que la huella detecta una escritura en NTFS.

## Punto 3, recuperación: elegibilidad antes de cortar — 2026-11-24

La parte de BM25/kNN del punto 3 del plan, que el ADR-0038 pone como
condición antes de construir la puerta de Aetheris. Las dos ramas cortaban su
lista antes de quitar lo que no podía salir: BM25 pedía `10 × limit`
fragmentos (al menos 50) y kNN 200, y la papelera y la deduplicación por
documento se aplicaban después, en Rust; la superficie quitaba además la
identidad pendiente con la lista ya cortada. Un documento largo o unos
cuantos de la papelera arriba dejaban la lista corta o vacía teniendo
respuesta.

Ahora «lo que puede salir» se escribe una vez, en SQL, en
`obrain-core/src/elegibilidad.rs`, con dos modos: `Ventana` (lo de siempre:
nada pendiente de publicar, ruta vigente, identidad sin marca pendiente) y
`SoloNormal` (lo que pide el ADR-0038, B5 y ajuste 3: `sensitivity =
'normal'`, publicación `ready`, ruta vigente e identidad confirmada, sin
depender del PIN). La ruta vigente en SQL es la misma condición bitemporal y
la misma precedencia que `Store::resolve`, con una prueba que las compara.
BM25 filtra y se queda con el mejor fragmento de cada documento dentro de la
consulta, y su `LIMIT` cuenta documentos; kNN filtra en el `WHERE` y
deduplica antes de truncar. La ventana usa `Ventana` y no cambia lo que
enseña; `SoloNormal` todavía no tiene llamante: la puerta no se construye
aquí.

Pruebas con bibliotecas sintéticas en memoria, en las dos ramas: un
documento con más fragmentos que el antiguo tope no tapa al siguiente; lo de
la papelera, lo pendiente de publicar y lo de identidad pendiente no ocupan
puesto; una colección con lo de dentro en la papelera; la frase parcial de
rescate; y diez sensibles por delante de un normal con `SoloNormal`, más un
documento sin fila de publicación, que no cuenta como clasificado. Cada una
falló con una mutación temporal que devolvía el comportamiento anterior (sin
filtro o cortando antes de deduplicar). Las pruebas cazaron además un error
propio: una columna sin tabla dentro de `content_publication p` se resolvía
como `p.entity`; ahora la expresión exige la columna con su tabla.

Queda del punto 3 la reconciliación de duplicados, versiones y sesiones.

**Precisión, 2026-11-24 ([ADR-0039](adr/0039-la-puerta-de-aetheris-se-adelanta.md)).**
Los duplicados (`duplicate_of`) se reconcilian desde `662bdee`; lo que
queda son versiones (`version_of`) y sesiones (`occurred_during`), que no
retractan una arista que deja de cumplirse. No bloquean la puerta: su primera
versión de `documento` no trae relaciones inferidas. Al revisarlo salió un
hueco que no estaba en el plan: la sensibilidad no sigue a las copias, así
que la copia sin marcar de algo protegido queda `normal`. Visto leyendo el
código, sin reproducirlo; la puerta no se enciende sin la regla que deja
fuera esas copias y su prueba.

## P1 identidad entre bibliotecas

Dos regresiones nuevas de pipeline con archivos sintéticos y base en memoria
fallaron antes del arreglo: A→B y B→A daban al archivo movido otro `EntityId`
porque el reemplazo de la ruta heredaba el original. Las pruebas comparan
también sensibilidad, corrección de caducidad, colección, relación, historial,
papelera e idempotencia. Pasan después del inventario global. Otros casos
cubren raíz ausente, raíces solapadas, candidato sin `FileID`, `FileID`
duplicado, recorrido limitado, salida de búsqueda/presentación y recuperación
tras aplazamiento, además de un reemplazo entre recorrido y verificación. El guardado atómico y la recuperación desde papelera siguen
cubiertos por las pruebas existentes.

En esta rama se ejecutaron `cargo test --locked --workspace`, `cargo clippy
--locked --workspace --all-targets -- -D warnings`, `cargo check --locked
--manifest-path ../ui/src-tauri/Cargo.toml`, `npm test`, `npm run check` y
`npm run build` sobre fixtures o compilación, sin abrir la bóveda personal.

## Alcance y decisiones

Checkout inicial `c183f3c`, limpio, en la rama de desarrollo existente.
Trabajo en `codex/brain-confianza`, sin reset ni descarte de cambios. No había
un PR abierto apropiado. Se conserva la arquitectura y no se añaden integraciones.

Una entrega de la primera etapa con commits separados para núcleo/puente,
cliente y documentación. La base del PR es
`claude/orchestrators-brain-fase-3-wdiheq`: contiene el producto auditado; apuntar
directamente a `main` mezclaría el desarrollo previo con esta corrección.

Cambios: lista cerrada de correcciones y objetos tipados; confirmación por ID;
autorización de sujeto/destino y comandos alternativos; savepoints y proyección;
URL limitada al origen; filtrado de metadatos antes de serializar; apertura
sólo por ruta determinista con cerrojo; plazo fijo del PIN y revocación local;
descarte de respuestas antiguas y retirada de caches/claves. Purga de vectores
`vault` sin modelo y filtro en kNN incluidos como dependencia de seguridad.

La revisión detectó otra frontera: dos decisiones de sensibilidad con el mismo
timestamp podían mantener vigente la anterior. La decisión del usuario ahora
es única, aunque las señales del clasificador sigan siendo múltiples.

## Evidencia antes/después

Las primeras cinco pruebas de `confianza.rs`, sobre el código anterior,
fallaron por aceptación de ruta interna, tipo incorrecto, salto del PIN,
metadatos protegidos serializados y sensibilidad que permanecía normal.
Pasan con el arreglo. La confirmación de relaciones conserva el ID del objeto,
es idempotente y rechaza ID ajeno/retractado; no se borran inferencias.

Dos regresiones compilan y ejecutan el estado real de Svelte con transporte
sintético: el estado de `c183f3c` concede acceso tras fallar seguridad y no
revoca antes de esperar al bloqueo; ambas fallan en modo histórico y pasan
con el estado nuevo. El resto comprueba caducidad, contenido recibido, petición
y consulta de seguridad en vuelo, bloqueo fallido/pendiente, desbloqueo posterior
y lectura legítima sin PIN.

La comparación adicional usa una copia temporal de `git archive c183f3c`, sin
abrir datos persistentes: misma regresión de decisiones simultáneas, origen
guardado en un flujo NTFS sintético y vector `vault` sin purgar.
Las tres fallan sobre el código histórico por el motivo esperado y pasan
sobre el código corregido. La comparación compartió inicialmente artefactos
de compilación y produjo un error de resolución de crate en un doctest;
se repitió la validación actual sin compilaciones concurrentes. En futuras
comparaciones usar un `CARGO_TARGET_DIR` separado para cada checkout.

Reproducción de las dos regresiones UI históricas desde `ui/` (Node 24):

```powershell
npm test
$env:OBRAIN_BASELINE = '1'
node --test tests/app-security.test.mjs  # se esperan dos fallos
Remove-Item Env:OBRAIN_BASELINE
```

## Validación y límites

Validación del primer cierre (`873a7f9`), con lockfiles y en Windows:

| Comprobación | Resultado |
|---|---|
| `cargo test --locked --workspace` desde `core/` | 487 pruebas y 1 doctest aprobados |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Aprobado |
| `cargo fmt --check` en ambos workspaces | Aprobado |
| `npm test` desde `ui/`, Node 24 | 9 pruebas aprobadas |
| `npm run check` | 0 errores y 0 advertencias |
| `npm run build` | Aprobado |
| `cargo check --locked --manifest-path ../ui/src-tauri/Cargo.toml` desde `core/` | Aprobado; workspace independiente |
| `git diff --check` | Aprobado |

Se usó un directorio de compilación fuera del checkout por OpenSSL en Windows.
El enlazador emitió avisos de símbolos PDB ausentes en dependencias nativas;
las pruebas y compilaciones terminaron correctamente.

Los tests Rust usan memoria o carpetas temporales; el caso NTFS crea y elimina
un archivo sintético.
En ese primer cierre no se ejecutó la app. No se abrió la bóveda real, ni se reindexó, migró o saneó
información personal. No se verificó una restauración de la bóveda real.

No se ha probado la interacción visual de la app empaquetada, OCR documental
real, calidad E5 sobre corpus representativo, batería ni bibliotecas grandes.
Persisten los dos paquetes npm con avisos altos; el alcance inspeccionado está
en `AUDITORIA-CONFIANZA.md`. No se aplicó una actualización de dependencias npm.

La política histórica de retención queda preparada como propuesta y procedimiento;
no se pide aprobación para ella en esta entrega porque no se ejecuta ninguna
operación sobre datos existentes. No hay cambio de esquema ni de criptografía.

## Revisión independiente de la PR #1

Se confirmaron ambos hallazgos sobre `873a7f9`: clasificación sin modelo con
un vector todavía almacenado y corrección manual que eliminaba vectores de
parte/paquete dejando `embedded_with` vigente. Las regresiones se ejecutaron
antes y después del arreglo; los fallos previos fueron los esperados, no de
infraestructura. La prueba de la clasificación y la del canal Brain pasan
con el arreglo.

Decisión: revocación central en `Store::set_sensitivity(id, level, at)`;
procedencia y vectores comparten savepoint. Herencia de carpetas/partes,
clasificación automática, CLI y Brain usan la misma política. Se eliminan
las purgas SQL duplicadas y M4 usa el mismo reconciliador para residuos,
incluida procedencia sin vectores. La clasificación escribe todas las señales
antes de proyectar; paquete procesado primero e informe final ven la misma
clasificación de sus partes. No se cambia el esquema ni se crea el índice
opcional desde core. Ver ADR-0035.

Pruebas nuevas: 15 casos en `obrain-embed/tests/revocacion.rs` y un pipeline
completo en `obrain-api/tests/revocacion.rs`, además de ampliar el caso Brain
existente. El pipeline vectoriza con un embebedor sintético, renombra el archivo
sin cambiar texto, reindexa con `None` y comprueba purga y retractación sin
reextracción. Se verifica conservación del historial, idempotencia, dry-run,
ausencia de tabla opcional, herencia y orden de paquete/parte, rollback ante
fallo de borrado/retractación y retorno legítimo a ambos niveles permitidos.
Se generan vectores/aserciones nuevos; no se reactivan los antiguos.

Validación actual: 503 pruebas Rust y 1 doctest; fmt y Clippy con `-D warnings`;
10 pruebas cliente, check Svelte sin errores/advertencias y build frontend.
Se construyó también Tauri nativo debug con assets incorporados, sin instalador.
La prueba nativa detectó un tercer problema: los timers de Window se invocaban
con `PinSession` como receptor. Node lo toleraba; en WebView el estado permanecía
desconocido. Se enlazan al objeto global y se añadió una regresión con ese
contrato de receptor: falla antes y pasa después. El panel real ya confirma
PIN activo y el bloqueo explícito retira texto/fecha recibidos.
En esa instancia nativa, texto y fecha visibles se retiraron al bloquear.
Tras otro desbloqueo, la imagen coral y la fecha estaban visibles a los 13 s;
la búsqueda se consultó a los 40 s. Sin volver a autenticar, a los 415 s la
imagen estaba sustituida por «Protegido con tu PIN» y la fecha por «fecha
protegida con tu PIN». No se muestreó el instante exacto de los 300 s; ese
límite y las carreras se cubren con reloj/transporte controlados. La búsqueda
seguía abierta con nombres públicos; sus fragmentos sensibles ya estaban
ocultos antes de caducar, por lo que no se atribuye a esta inspección visual
una prueba de retirada de fragmentos recibidos por IPC. El panel Seguridad
confirmó «Bloqueado» después; se cerró sólo la instancia del fixture.
Se usaron sólo dos documentos sintéticos, base/llave nuevas y un perfil WebView
independiente, sin modelo. No se verificó MSI/NSIS release ni caducidad tras
permanecer cinco minutos en segundo plano.
Se conserva [un procedimiento manual](VALIDACION-PIN-AISLADA.md) que aísla base,
bibliotecas, resolución de modelos y datos WebView; no usa la bóveda personal.

## Siguiente sesión tras la revisión

Revisar el PR, después identidad entre bibliotecas y publicación tras
clasificación. Continuar con relaciones obsoletas, completitud de búsqueda y
persistencia/recuperación. Retener la visión de Jarvis mediante mejoras pequeñas
de utilidad comprobable y memoria explícita ligada a fuentes.
