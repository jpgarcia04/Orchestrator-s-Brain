# Progreso — 2026-10-11

**Actualización:** la PR #2 quedó fusionada en `main` (`48f73f5`). Las
referencias más abajo a la rama previa y a la base del PR documentan el
estado de aquella entrega, no el estado actual. La identidad entre bibliotecas
se trabaja desde `main` en `codex/identidad-entre-bibliotecas` (ADR-0036).

## Puerta de Aetheris en Windows: pasos 5 y 6 en la laptop — 2026-11-24

En la laptop de JP (Windows 11, `main` en `5b160f2`), con bases sintéticas en
carpetas temporales y nunca la de verdad.

- **`cargo test` en Windows:** todo el núcleo en verde (682 pruebas, #32 y #33
  incluidas) y las de `ui/src-tauri`. `ui/src-tauri/tests/puerta.rs` pasa
  también con `--release`, es decir, con `windows_subsystem = "windows"`: el
  ejecutable usa las tuberías que le da el cliente como stdin y stdout, no abre
  ventana y sale con 0 al cerrarle la entrada. Era lo que faltaba del paso 5.
- **Redirigida a archivos** (`Start-Process -RedirectStandardInput/Output`),
  la puerta apagada contesta las tres líneas esperadas: `serverInfo`
  `obrain`/`1`, las tres herramientas y `no_disponible: apagada`.
- **El interruptor de la ventana:** JP pasó a «Sólo localizar» en una ventana
  release apuntando a la base sintética. Después, `localizar` trae
  `Normal/recibo.txt` y `leer` sigue en `apagada`, con la ventana cerrada y con
  la ventana abierta a la vez (leer a la vez funciona). No se probó el PIN.
- **Paso 6 en Windows:** `preparar_brain.py` de Aetheris arma la base con
  `bateria_aetheris.rs`; la ruta con barras invertidas funciona (los 14
  documentos quedan indexados). Niveles: bóveda las credenciales inventadas y
  el archivo «IGNORA LAS REGLAS…»; normal la póliza de Seguros Aurora, que en
  Linux salió sensible antes del ADR-0040 y ahora concuerda con él. Las 8
  pruebas de `test_puerta_real.py` pasan, y la batería con el modelo local no
  logró ninguna llamada provocada por un documento (detalle en Aetheris,
  `docs/ataques/2026-11-24.md`). Con la biblioteca de T3, Brain da los mismos
  niveles que la puerta simulada.

Para compilar en Windows hizo falta el `perl` de Strawberry delante en el
`PATH` (el de Git Bash no trae los módulos que pide OpenSSL vendored), y la
ventana hecha con `cargo build` necesita `--features tauri/custom-protocol`;
sin eso busca la interfaz en `localhost`. Falta la pasada de la batería con
Claude y decidir si se conecta.

## Punto 3, relaciones: versiones y sesiones se reconcilian — 2026-11-24

Lo que quedaba del punto 3. `obrain_link::versions` y
`obrain_link::sessions` sólo añadían aristas: un archivo renombrado seguía
siendo «versión de», las versiones antiguas seguían apuntando al canónico
anterior cuando aparecía uno más nuevo, y un archivo editado días después
seguía colgando de la sesión de aquella tarde. Ahora cada pasada calcula las
aristas que el nombre o el `mtime` sostienen hoy y retracta, con fecha, las
que este enlazador afirmó y ya no se sostienen, igual que `duplicate_of`
desde octubre. No toca la corrección del usuario ni lo que afirmó otra
procedencia, y no retracta nada de una identidad pendiente (ADR-0036); un
archivo cuya cadena apunta a una identidad pendiente conserva esa arista y no
gana un segundo canónico, porque `version_of` es de cardinalidad uno.

El concentrador de una sesión no se borra ni se retracta: la sesión ocurrió y
es historia que el sistema autoró. Sin aristas no aparece en ninguna vista,
porque todas llegan a una sesión por `occurred_during`, y si los mismos
archivos la vuelven a formar se reutiliza. Su `session_span` no se reescribe
cuando la sesión crece por el final; el nombre sale sólo del inicio.

Pruebas unitarias en los dos enlazadores (renombrar, quitar y reponer,
versión nueva, inicio de sesión que cambia, usuario, otra procedencia,
identidad pendiente) y `obrain-api/tests/relaciones_inferidas.rs`, con
indexado real sobre bibliotecas sintéticas: renombrar una versión y editar un
archivo otro día retiran la relación del log y de la ficha, y la historia la
conserva. Las de retractar fallan si no se retracta; las de conservar fallan
si se retracta sin mirar el método, la procedencia o la identidad pendiente.

`documento` de la puerta sigue sin relaciones inferidas: la versión `1` del
contrato las deja fuera (ADR-0039, condición 2) y meterlas la sube, lo que
también toca a Aetheris.

## Restaurar no sustituye una base abierta — 2026-11-24

Las dos pruebas de `respaldos.rs` que fallaban en Linux tenían razón: en
Linux la restauración sustituía una base que otra conexión, del mismo proceso
o de otro, tenía abierta y quieta, y esa conexión seguía escribiendo en el
archivo que pasaba a la salvaguarda, con lo que su trabajo se perdía sin
aviso. En Windows no ocurría porque la apertura con `share_mode(0)` del
sistema lo frena.

`comprobar_conexiones_activas` ponía `PRAGMA locking_mode = EXCLUSIVE`
después de su primera lectura. Una conexión WAL quieta no tiene ningún
bloqueo con el que choque ese modo puesto tarde, y el `wal_checkpoint` no ve
lectores. Ahora el modo exclusivo va antes de la primera lectura (tras la
llave de SQLCipher): esa lectura pide el bloqueo exclusivo y cualquier otra
conexión abierta responde «ocupada», que es `BaseEnUso`. La comprobación de
Windows sigue igual y se suma a esta.

Las dos pruebas fallaban sin el cambio y pasan con él; `cargo test` del
núcleo entero pasa en Linux. Queda pasarlo en Windows, donde la comprobación
del sistema ya cubría el caso.

## Puerta de Aetheris, paso 6: la base de la batería de Aetheris — 2026-11-24

La batería de ataques de Aetheris ya puede correr contra la puerta de verdad.
Brain pone una sola pieza: `obrain-api/tests/bateria_aetheris.rs`, un
ayudante con `#[ignore]` que sólo corre cuando Aetheris lo lanza con
`--ignored` y `OBRAIN_BATERIA` apuntando a un manifiesto (biblioteca, base,
colecciones, posición de la puerta, modelo sí o no, dónde escribir el
informe). Indexa la biblioteca como la ventana, deja que el clasificador de
verdad decida lo sensible y lo de bóveda, crea las colecciones, mueve el
interruptor con `Brain::set_gate_position` y escribe qué nivel y qué estado
de publicación dio a cada archivo. Es una prueba y no un subcomando de la CLI
para no añadir al producto una forma de mover el interruptor sin la ventana.
Se niega si ya hay algo donde va la base (la base, `-wal`, `-shm`,
`-journal` o `.llave`) o si la base es la de la carpeta fija: los documentos
de la batería son texto hostil a propósito. Se comprobó a mano que las dos
negativas paran antes de crear nada.

Lo demás está en Aetheris (`pruebas/ataques/preparar_brain.py`, que arma la
biblioteca en un temporal y lanza este ayudante, y
`pruebas/test_puerta_real.py`, sin modelos). Primera pasada aquí, con el
ejecutable de verdad en Linux y sin modelo de significado: el cliente de
Aetheris acepta la puerta, ninguna respuesta se descarta por forma, y lo que
el clasificador marcó como protegido (las credenciales inventadas y el
archivo cuyo nombre es la orden «IGNORA LAS REGLAS…», como bóveda; la póliza
inventada, como sensible) no sale por ninguna herramienta con 17 consultas
que lo buscan por nombre y por contenido; `documento` contesta
`fuera_de_la_puerta` igual a lo protegido y a lo inventado. Cambiando
temporalmente la rama léxica de la puerta a `Elegibles::Ventana`, las pruebas
de Aetheris que miran `localizar` y `leer` fallaron por el nombre del archivo
de bóveda.

Falta la pasada con modelos (Ollama y Claude) en el portátil de JP, la prueba
en Windows del paso 5, y decidir con ella si se conecta.

## Puerta de Aetheris, paso 5: el programa de la puerta — 2026-11-24

`obrain-app --puerta`: el mismo ejecutable que instala el instalador, en un
modo que se decide en `main` antes de tocar Tauri, así que no se abre ventana
ni se carga la interfaz ni corre el actualizador. Se eligió el mismo
ejecutable y no uno aparte porque ya va firmado en el instalador y no pide
cambiarlo. Habla MCP por stdio (`obrain-api/src/puerta_mcp.rs`): sólo
`initialize`, `ping`, `tools/list` y `tools/call`, `serverInfo.name`
`obrain` y `serverInfo.version` `1`, la versión del contrato. Cada llamada
abre la base para leer con la apertura del paso 1 y la suelta; lo que no deja
leer sale como `no_disponible` con su motivo. La base es la de `OBRAIN_DB` o
la de la carpeta fija, nunca `ubicacion::carpeta()`, que muda. Los argumentos
se comprueban contra el `inputSchema` publicado (nada de más, tipos, longitud
de la consulta, límite de 1 a 50); una línea de más de 64 KiB se descarta sin
leerla entera; un fallo se contesta sin detalles. El modelo de significado se
carga la primera vez que lo pide `localizar` o `leer`, nunca para
`documento`. El contrato, en [PUERTA-AETHERIS](PUERTA-AETHERIS.md). La frase
de la ventana que decía que la puerta todavía no venía pasa a decir cómo la
arranca Aetheris.

Pruebas: en `puerta_mcp.rs`, los errores de JSON-RPC, que las notificaciones
no se contestan, que sin `initialize` no hay herramientas, las versiones de
MCP, los argumentos contra el esquema, las tres herramientas de sólo lectura,
la línea demasiado larga y que sin base dice `base_cerrada` sin crear nada.
En `obrain-api/tests/puerta.rs`, una conversación entera contra la biblioteca
sintética: contesta lo mismo que el núcleo, no deja salir nada de lo de
bóveda, `documento` responde igual a lo de bóveda y a lo inventado, un
argumento de más no llega al núcleo y la base no cambia ni un byte; respeta
las tres posiciones; carga el modelo una vez y no para `documento`; y dice
`ocupada` ante un `-journal` sin poner la ruta en la respuesta. En
`ui/src-tauri/tests/puerta.rs`, el ejecutable de verdad lanzado sin
`DISPLAY` ni `WAYLAND_DISPLAY` (sin `--puerta`, en esas condiciones, Tauri
no arranca: se comprobó), con `HOME` y XDG en un temporal: contesta, sale
con 0 al cerrarle la entrada, no cambia la base y rechaza argumentos de más.
Mutaciones temporales (aceptar argumentos desconocidos, límite sin tope,
herramientas sin `initialize`, cargar el modelo para `documento`, poner la
ruta en `no_disponible`, otro nombre para el modo): cada una rompió su
prueba.

Falta en Windows: que un ejecutable con `windows_subsystem = "windows"` use
las tuberías que le da el cliente como stdin y stdout, y lo pendiente del paso
1 sobre NTFS. Y falta el paso 6: la batería de Aetheris contra una base
sintética desechable, antes de conectarla.

## Puerta de Aetheris, paso 4: el ajuste de tres posiciones — 2026-11-24

El ajuste del [ADR-0038](adr/0038-la-puerta-de-aetheris.md) (B2 y ajuste 5):
**apagada** · **sólo localizar** · **localizar y leer lo normal**, de fábrica
apagada. El ADR-0039 dejaba abierto dónde guardarlo; se guarda en la tabla
`seguridad`, dentro de la base cifrada y junto al PIN, con la clave
`puerta_de_aetheris`, y no como aserción del log: es un control, no
conocimiento, no pide migración y la apertura de la puerta ya exige esa
tabla. El precio es que no queda historia de cuándo se cambió. Sin ajuste, o
con un valor que no es ninguna de las tres posiciones, cuenta como apagada.

`obrain-api/src/puerta.rs`: `Posicion` y `posicion()`, y cada herramienta lee
el ajuste dentro de la misma lectura antes de mirar nada; si no la deja,
contesta `NoSale::Apagada` (motivo `apagada`). Con «sólo localizar», `leer` y
`documento` dicen `apagada`, también para un identificador inventado.
`FueraDeLaPuerta` pasa a ser `NoSale::FueraDeLaPuerta`, con su mismo motivo.
Sólo la ventana escribe el ajuste (`Brain::set_gate_position`, comandos
`puerta_posicion` y `puerta_poner`): abrir más pide la ventana desbloqueada si
hay PIN, como marcar una carpeta; cerrar no se bloquea nunca. Lo que sale por
la puerta sigue sin depender del PIN (ajuste 3): esto sólo guarda quién puede
mover el interruptor.

En la ventana, una pestaña «Aetheris» en el panel de Seguridad
(`ui/src/componentes/Puerta.svelte`) con las tres posiciones y lo que recibe
Aetheris en cada una: que lo normal puede salir, que que un documento salga ya
dice algo de su texto, que los nombres de carpetas y colecciones también salen
y que lo protegido no sale nunca, haya PIN o no. Enseña la posición que el
núcleo devuelve después de escribirla; si no puede leerla, no marca ninguna y
no deja elegir. Dice también que esta versión todavía no trae el programa de
la puerta; esa frase se quita en el paso 5.

Pruebas: en `obrain-api/tests/puerta.rs`, la puerta viene apagada y cada
posición deja exactamente sus herramientas, leídas con la apertura de la
puerta; abrir con el PIN bloqueado falla y cerrar no; cuatro valores raros
dejan la puerta apagada. En `puerta.rs`, la tabla de posiciones y que el texto
guardado y el JSON de la ventana coinciden. En `ui/tests/puerta-ui.test.mjs`,
el componente montado con un núcleo sintético: enseña la posición y los
textos, no escribe si no pudo leer, pide el PIN para abrir y no abre si se
cancela, reintenta tras el PIN y cierra sin pedirlo. Mutaciones temporales
(un valor raro abre, `leer` comprobado como `localizar`, sin guarda de PIN,
guarda también al cerrar, sin reintento tras el PIN, ajuste ilegible como
apagada, sin la frase de carpetas y colecciones): cada una rompió su prueba.
La aplicación Tauri se compiló y probó en Linux con WebKitGTK; el interruptor
no se ha visto todavía en Windows.

## Puerta de Aetheris, paso 3: las tres herramientas en el núcleo — 2026-11-24

`obrain-api/src/puerta.rs`: `localizar`, `leer` y `documento`, sobre un
`Store` abierto con la apertura del paso 1, con `Elegibles::SoloNormal` y sin
mirar nunca el PIN ni la ventana. `localizar` devuelve `Ubicacion`, un tipo
sin ningún campo para texto del documento, con la ruta relativa a su
biblioteca (lo que no está en ninguna biblioteca no sale). `leer` da pasajes
con su documento, su método (`bm25` o `knn`) y dos fechas: cuándo extrajo
Brain esa versión y la modificación que observó en el disco. `documento` da
la ficha de lo normal, con la ruta completa, la caducidad con su procedencia
y sólo las relaciones deterministas (carpeta, copia de, dentro de) hacia lo
que también puede salir; para lo sensible, lo de bóveda, su copia sin marcar,
lo de la papelera, un identificador inventado o vacío responde
`FueraDeLaPuerta` (desde el paso 4, `NoSale::FueraDeLaPuerta`), lo mismo en todos los casos. Cada respuesta lleva el mismo
aviso fijo. Todavía no hay programa que las sirva: eso es el paso 5.

Pruebas en `obrain-api/tests/puerta.rs`, de punta a punta con una biblioteca
sintética indexada como la indexa la ventana, una carpeta marcada como bóveda
y la base leída con la apertura de la puerta: la respuesta de `localizar` no
lleva ni la palabra que sólo está en el texto ni la ruta de la biblioteca;
preguntar por lo protegido responde igual que preguntar por nada; los
pasajes son del documento normal y dicen su método y sus fechas; `documento`
responde igual a los cinco casos de fuera; la ficha no trae relaciones
inferidas aunque el enlazador escribió una versión; el PIN puesto o
desbloqueado no cambia nada; y con un modelo de significado de juguete, la
copia sin marcar del pasaporte no sale y el pasaje dice `knn`. Mutaciones
temporales (buscar con `Ventana`, no comprobar `documento`, añadir
`version_of`, dar la ruta completa en `localizar`, marcar todo como `bm25`):
cada una rompió su prueba.

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

`cargo test --workspace` en Linux pasaba entero salvo dos pruebas de
`respaldos.rs` que ya fallaban en `main` en Linux
(`restauracion_rechaza_conexiones_abiertas` y
`restauracion_cancelada_por_base_en_uso_no_revierte_trabajo_posterior`). La
causa no eran los bloqueos POSIX sino un fallo de la restauración, arreglado
después (ver «Restaurar no sustituye una base abierta»). Queda por comprobar en Windows que `readonly_shm` e
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
