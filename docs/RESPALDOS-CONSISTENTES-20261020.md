# Respaldos consistentes, verificables y ensayables — 20 de octubre de 2026

## 1. Enfoque elegido y justificación

Los perfiles de Orchestrator's Brain están siempre cifrados con SQLCipher 4 (`brain.obrain` + `brain.obrain.llave`) y operan en modo WAL (`PRAGMA journal_mode = WAL`).

Durante la inspección técnica se comprobó que `Store::export_encrypted` **no era reutilizable directamente** para respaldar bases activas:

1. Exige que el origen esté en claro (`verify_existing(origen, None)`) porque fue diseñada para la migración inicial de bases sin cifrar.
2. Ejecuta `PRAGMA wal_checkpoint(TRUNCATE)`, que muta el WAL activo y falla cuando otra conexión mantiene una lectura abierta (`checkpoint_ocupado_no_se_presenta_como_completo`).

En lugar de introducir un formato propietario de bóveda o criptografía adicional, se eligió **`VACUUM INTO`** sobre la versión integrada de SQLCipher 4.6.1 (`libsqlite3-sys 0.35.0`), encapsulado en `Store::export_snapshot_encrypted`:

- **Instantánea transaccional sin truncar el WAL activo**: `sqlite3RunVacuum` abre una única transacción de lectura (`sqlite3BtreeBeginTrans(pMain, 0, 0)`) sobre la base origen y lee de forma consistente todas las páginas confirmadas tanto en `brain.obrain` como en `brain.obrain-wal`, conviviendo con lectores y escritores concurrentes.
- **Herencia de cifrado sin paso en claro**: al adjuntar internamente `vacuum_db` con ruta de archivo y clave vacía, SQLCipher copia el `keyspec` de 32 bytes de `main` y cifra cada página del destino en modo `journal_mode = DELETE` (un único archivo consolidado, sin `-wal` ni `-shm`). Con `PRAGMA temp_store = MEMORY`, ninguna página temporal sin cifrar toca el disco.
- **Publicación atómica y sin sobrescritura**: `obrain_api::respaldo::crear` construye el conjunto en un directorio temporal privado (`.obrain-respaldo-tmp-*`) junto al destino, verifica el respaldo completo antes de publicarlo y sólo entonces lo renombra atómicamente a la carpeta final. Si el destino ya existe y no está vacío, falla cerrado (`DestinoOcupado`).

Un respaldo publicado contiene exactamente tres archivos:

```text
<carpeta-respaldo>/
├── brain.obrain         instantánea SQLCipher consolidada (sin WAL)
├── brain.obrain.llave   caja del sistema (DPAPI) + caja de recuperación (Argon2id + XChaCha20-Poly1305)
└── manifiesto.json      versión, tamaños, hashes BLAKE3 y resumen canónico de la instantánea
```

## 2. Qué garantiza realmente el respaldo y qué no

- **Sí garantiza**:
  - Consistencia transaccional completa de la instantánea SQLite/SQLCipher en el instante en que `VACUUM INTO` inicia su lectura, incluyendo todas las transacciones ya confirmadas en el WAL.
  - Integridad de páginas autenticadas (`PRAGMA cipher_integrity_check`), integridad estructural de índices y B-trees (`PRAGMA integrity_check`), versión de esquema (`user_version`), tablas y triggers de los invariantes I1 e I4.
  - Correspondencia mutua entre `brain.obrain`, `brain.obrain.llave` y `manifiesto.json`: el resumen y la `huella_contenido` del manifiesto se calculan **leyendo directamente la instantánea `staged_db` ya generada en modo solo lectura**, nunca mediante una segunda consulta a la base activa (evitando desfases si otra conexión escribe en paralelo).
  - Correspondencia criptográfica entre la llave de datos que abre `brain.obrain`, la caja de recuperación de `brain.obrain.llave` y la clave de recuperación registrada en la tabla `seguridad` (`obrain_keys::verify_recovery_matches`).
  - Rechazo de cualquier archivo ausente, vacío, truncado, modificado o auxiliar inesperado (`-wal`, `-shm`, `-journal`, `.cifrando`, etc.).
- **No garantiza por sí solo**:
  - **Frescura absoluta frente a una base activa posterior**: ningún manifiesto autocontenido puede demostrar que es el respaldo *más reciente* si la base activa siguió recibiendo escrituras después de tomar la instantánea y no existe una referencia independiente con la que comparar.
  - **Conservación de los archivos originales externos**: el respaldo contiene el cerebro indexado (log bitemporal, fragmentos de texto extraídos, índice FTS5, vectores, colecciones, sensibilidad, vencimientos y papelera), pero **no copia los PDF, imágenes o documentos originales** alojados en las carpetas de las bibliotecas del usuario.

## 3. Archivos modificados

- `core/obrain-keys/src/lib.rs`: añade `verify_recovery_matches` (comprueba que la caja `recuperacion` descifra con la clave de recuperación exactamente a los mismos 32 bytes de `DataKey` que abren la base) y `recovery_box_canonical` (valida tamaños y parámetros de `.llave` y produce una representación canónica no secreta de la caja de recuperación para el manifiesto), más prueba unitaria.
- `core/obrain-core/src/opening.rs`: añade `has_migration_artifacts`, `validate_integrity` (`validate` + `cipher_integrity_check` + `integrity_check`) y `verify_snapshot` (verifica una instantánea consolidada sin WAL sobre una copia temporal privada sin modificar el archivo inspeccionado).
- `core/obrain-core/src/store.rs`: añade `Store::verify_snapshot` y `Store::export_snapshot_encrypted`.
- `core/obrain-core/tests/cifrado.rs`: añade prueba de `export_snapshot_encrypted` y `verify_snapshot` con WAL activo y lector concurrente.
- `core/obrain-api/src/respaldo.rs`: implementa la creación transaccional con publicación atómica (`crear`, `crear_con_etapa`), verificación sin escrituras (`verificar`, `verificar_con_clave`) y ensayo aislado de recuperación y apertura (`ensayar`).
- `core/obrain-api/src/lib.rs`: expone `pub mod respaldo`, sus tipos principales (`Manifiesto`, `ResumenInstantanea`, `InformeEnsayo`) y `Error::Respaldo` (integrado con `Error::is_key_unavailable()`).
- `core/obrain-api/src/bin/obrain.rs`: incorpora en la CLI:
  - `obrain respaldar <carpeta>`
  - `obrain respaldo <carpeta> --verificar [--clave [<clave>|-] | --pedir-clave]`
  - `obrain respaldo <carpeta> --ensayar [--en <carpeta>] [--clave [<clave>|-] | --pedir-clave] [--pin [<pin>|-] | --pedir-pin]`
  - `obrain recuperar [<clave> | -]` (compatible con el argumento histórico, o con petición interactiva sin eco / stdin / `OBRAIN_CLAVE`)
  - Entrada segura de secretos por consola interactiva (suprimiendo el eco con `SetConsoleMode` en Windows), tubería stdin (`-`) y variables de entorno (`OBRAIN_CLAVE`, `OBRAIN_PIN`), evitando exponer claves o PINs en los argumentos del proceso o en el historial de comandos (`PSReadLine`).
- `core/obrain-api/tests/respaldos.rs`: suite de 10 pruebas de integración de extremo a extremo sobre bases ficticias y archivos temporales (incluyendo temporales abandonados y secretos seguros por CLI/entorno).

## 4. Pruebas ejecutadas y resultados

Comandos ejecutados desde `core`:

```powershell
cargo test --locked -p obrain-keys -p obrain-core -p obrain-api
cargo clippy --locked -p obrain-core -p obrain-keys --all-targets -- -D warnings
cargo clippy --locked -p obrain-api --lib --bins --test respaldos -- -D warnings
cargo fmt --check
git diff --check
```

- **Resultado global**: **279 pruebas ejecutadas y aprobadas, 0 fallos, 0 ignoradas** (`13` en `obrain-keys`, `41` + `1` doctest en `obrain-core`, `224` en `obrain-api`).
- **Escenarios verificados en `core/obrain-api/tests/respaldos.rs` y `core/obrain-core/tests/cifrado.rs`**:
  1. `respaldo_de_base_cifrada_realista_conserva_todo_al_abrir`: base cifrada con biblioteca, subcarpetas, marcas `sensitive` y `vault`, paquete con documento incluido (`part_of`), fecha de caducidad corregida por el usuario, colección del usuario, documento en papelera, fragmentos FTS5, vectores y PIN.
  2. `instantanea_durante_actividad_concurrente_es_consistente` y `exportar_instantanea_de_base_ya_cifrada_con_wal_activo_no_pierde_nada`: lector reteniendo instantánea en WAL + escritor concurrente insertando pares atómicos mientras se genera el respaldo, verificando que no se pierde el WAL ni se parte ninguna transacción.
  3. `detecta_archivos_ausentes_incompletos_modificados_o_desparejados`: archivos ausentes, vacíos (0 bytes), truncados, bytes alterados en `brain.obrain`, JSON roto o versión futura en `manifiesto.json`, archivos extraños (`-wal`, `.cifrando`, `nota.txt`), llave `.llave` cruzada de otra base (incluso recalculando hashes en el manifiesto) e instantánea desfasada respecto al manifiesto, además de bloqueo de sobrescritura (`DestinoOcupado`).
  4. `clave_de_recuperacion_o_pin_incorrectos_fallan_sin_alterar_archivos`: clave mal formada, clave de recuperación incorrecta y PIN incorrecto, comprobando que los hashes BLAKE3 del respaldo no cambian y no se publica carpeta de ensayo.
  5. `interrupcion_durante_generacion_no_publica_respaldo_ni_deja_temporales`: inyección de fallos en `TrasCopiarLlave`, `TrasExportarBase`, `TrasEscribirManifiesto` y `AntesDePublicar`, comprobando que no se publica el destino ni quedan carpetas `.obrain-respaldo-tmp-*`.
  6. `directorio_temporal_abandonado_se_rechaza_y_no_bloquea_nuevo_respaldo`: simula un temporal huérfano con archivos válidos (como tras un corte eléctrico o terminación abrupta sin RAII), verificando que `respaldo::verificar` y `respaldo::crear` lo rechazan explícitamente (`DestinoInvalido`) y que la presencia del temporal no impide publicar futuros respaldos legítimos en el mismo contenedor padre.
  7. `ensayo_aislado_en_otro_equipo_sin_dpapi_no_modifica_respaldo_ni_base_activa`: simulación de otro equipo donde la caja del sistema no puede abrirse (`SistemaNoDisponible`), verificando y ensayando la recuperación con `--clave` y `--pin` tanto en directorio temporal como con `--en <carpeta>`, sin modificar el respaldo ni la base activa.
  8. `comportamiento_cuando_faltan_los_documentos_originales_externos`: ensayo y reindexación cuando falta la carpeta raíz de la biblioteca y cuando falta un archivo individual (con posterior restauración en disco).
  9. `cli_obrain_respaldar_verificar_y_ensayar_extremo_a_extremo`: invocación del binario real `obrain` para crear, verificar y ensayar un respaldo sin tocar ni crear una base activa inexistente.
  10. `cli_obrain_secretos_por_stdin_y_variables_de_entorno`: verificación y ensayo por CLI recibiendo la clave de recuperación y el PIN vía tubería en stdin (`--clave -`, `--pin -`), mediante variables de entorno (`OBRAIN_CLAVE`, `OBRAIN_PIN`), y recuperación segura con `obrain recuperar -` / `OBRAIN_CLAVE` sin exponer secretos en los argumentos del proceso.

## 5. Evidencia de recuperación sintética satisfactoria

En los ensayos de recuperación aislada (`respaldo::ensayar` y `obrain respaldo <carpeta> --ensayar [--en <carpeta>]`):

- Cuando la caja del sistema del archivo `.llave` proviene de otro equipo y no puede abrirse localmente, `verificar_con_clave` valida los hashes, el formato canónico de la caja de recuperación y descifra la llave en memoria para comprobar `cipher_integrity_check`, `integrity_check` y la `huella_contenido` sin escribir un solo byte en el respaldo.
- A continuación, `ensayar` copia `brain.obrain` y `brain.obrain.llave` a un directorio aislado, ejecuta `seguridad::recuperar` sobre esa copia (reenvolviendo la caja `sistema` con DPAPI local), abre `Brain::open`, verifica el bloqueo por PIN antes de autenticar y el acceso completo tras `unlock(pin)` (4 documentos en bóveda, 1 en papelera, 1 colección con 2 documentos, 1 paquete con su parte, 2 carpetas marcadas, fragmentos FTS5 y vectores), manteniendo intactos los hashes de la base activa y del respaldo original.

## 6. Seguridad del material de claves y archivos temporales

- **Sin copias en claro en disco**: `export_snapshot_encrypted` cifra directamente página a página en el archivo de destino con la misma `DataKey` de 256 bits y fija `PRAGMA temp_store = MEMORY`. Ningún texto documental, ruta ni secreto aparece en claro en el disco.
- **Cero secretos o metadatos personales en `manifiesto.json`**: el manifiesto sólo almacena recuentos numéricos, tamaños, versión de esquema y hashes BLAKE3 (`blake3` de archivos, `huella_recuperacion` de parámetros de la caja Argon2id y `huella_contenido` del estado interno). No contiene rutas de bibliotecas, nombres de documentos, fragmentos de texto, PIN ni la clave de recuperación.
- **Limpieza RAII y garantías ante terminación abrupta**:
  - Los directorios de *staging* (`.obrain-respaldo-tmp-*`) y de ensayo temporal limpian sus archivos en `Drop`.
  - Ante una terminación forzada (corte de corriente, `taskkill` o `SIGKILL` donde `Drop` no ejecuta), el directorio temporal queda inerte: ningún comando ni API permite verificarlo o ensayarlo como respaldo publicado (prefijos `.obrain-respaldo-tmp-` y `.obrain-ensayo-tmp-` quedan reservados y vetados).
  - Cada directorio temporal incluye 12 bytes aleatorios criptográficos (24 caracteres hexadecimales vía `getrandom`), garantizando que temporales abandonados no colisionen con respaldos posteriores.
  - La carpeta de destino final solicitada no se crea ni se publica hasta que el respaldo ha completado todas sus fases y ha sido verificado con éxito.
- **Entrada segura de credenciales en CLI**: la clave de recuperación y el PIN pueden solicitarse de forma interactiva sin eco por consola (`--pedir-clave`, `--pedir-pin`), por tubería estándar (`--clave -`, `--pin -`, `obrain recuperar -`) o mediante variables de entorno (`OBRAIN_CLAVE`, `OBRAIN_PIN`), evitando la exposición en el historial de PowerShell (`PSReadLine`) o en la lista de procesos del sistema operativo.
- **Clave de recuperación fuera del respaldo en claro**: dentro de `brain.obrain`, la clave de recuperación vive cifrada por SQLCipher; en `brain.obrain.llave`, la llave de 256 bits vive envuelta por DPAPI (`sistema`) y por Argon2id + XChaCha20-Poly1305 (`recuperacion`). Para abrir el respaldo en otro equipo donde DPAPI no esté disponible sigue siendo imprescindible que la persona conserve su clave de recuperación de 25 caracteres (y su PIN si desea desbloquear contenido sensible sin restablecerlo).

## 7. Tratamiento de documentos originales externos

- **Qué contiene el respaldo**: el log bitemporal completo (`entities`, `assertions`, `predicates`), el estado de publicación (`content_publication`), los fragmentos normalizados e índice léxico (`chunks`, `chunks_fts`), los embeddings (`vectors`) y la configuración de seguridad (`seguridad`).
- **Qué depende de copias externas**: los archivos originales en las carpetas de las bibliotecas del usuario (PDF, imágenes, documentos de texto).
- **Comportamiento al abrir o reindexar sin los originales**:
  - Sin reindexar, o al reindexar cuando la carpeta raíz de la biblioteca **no existe en el disco** (`!root.is_dir()`), `indexar::reindex` registra la biblioteca en `missing` y **se detiene sin tocar el log ni enviar ningún documento a la papelera** (`trashed == 0`). La búsqueda léxica/semántica, la vista previa de texto (`preview` lee de `chunks`), el grafo, las colecciones y los vencimientos siguen funcionando con normalidad; únicamente la apertura del archivo externo (`path_to_open`) y las miniaturas de imágenes locales devuelven `None` de forma segura.
  - Si la carpeta raíz de la biblioteca **sí existe** en disco pero faltan archivos individuales dentro de ella al ejecutar `reindex`, esos archivos pasan a la papelera de 30 días (`missing_since`), conservando sus fragmentos durante 30 días y sus aserciones y colecciones en el log; si los archivos originales se restauran en el disco, `bring_back` los reincorpora automáticamente por hash (`returned`) con todas las correcciones y colecciones del usuario intactas.

## 8. Limitaciones conocidas y trabajo pendiente para una restauración real

1. **No sustituye todavía la base activa**: conforme al alcance acordado, esta entrega sólo crea, verifica y ensaya respaldos en ubicaciones aisladas; no reemplaza `%USERPROFILE%\.obrain\brain.obrain`.
2. **Protocolo de sustitución segura pendiente**: para implementar la restauración real en una fase posterior habrá que:
   - Exigir que el respaldo supere `respaldo::ensayar` antes de tocar la base activa.
   - Detener el vigía de bibliotecas (`vigilar`) y cerrar la instancia activa de `Brain` / `Store` (incluidos sus descriptores `-wal` y `-shm`) antes de sustituir archivos.
   - Respaldar o apartar de forma reversible la base activa previa (junto con su `.llave` y su `-wal`) a una carpeta de salvaguarda antes de mover la base restaurada a su lugar, de modo que un fallo a mitad del reemplazo permita volver atrás sin pérdida.
3. **Sin referencia externa de frescura**: el manifiesto demuestra la integridad y consistencia de la instantánea respaldada, pero no impide que el usuario restaure voluntariamente un respaldo antiguo; la futura restauración deberá mostrar la fecha (`creado_en_ms`) y los recuentos frente a los de la base activa antes de confirmar.

## 9. Recomendación sobre el siguiente paso hacia la interfaz gráfica

1. **Subfase previa en el núcleo/CLI (restauración controlada)**: antes de añadir controles en Tauri, implementar y probar el reemplazo transaccional de la base activa con copia de salvaguarda automática de la base saliente y bloqueo de conexiones abiertas.
2. **Integración acotada en la sección «Seguridad» de Tauri**: una vez validada la sustitución en el núcleo, exponer en el panel de Seguridad de la aplicación dos acciones claras:
   - **«Crear respaldo…»** (selector de carpeta nueva → llama a `obrain_api::respaldo::crear`, muestra confirmación con recuentos y recuerda conservar la clave de recuperación y las carpetas de documentos originales).
   - **«Verificar / Ensayar respaldo…»** (selector de carpeta de respaldo → ejecuta `respaldo::ensayar` en segundo plano y presenta el `InformeEnsayo` indicando si las bibliotecas originales están presentes en este equipo).
