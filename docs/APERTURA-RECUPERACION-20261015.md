# Apertura y recuperación de bases existentes — 15 de octubre de 2026

1. **Estado inicial y base.** El checkout del usuario estaba en
   `codex/validacion-windows-v1`, commit `8817600`, sin modificaciones rastreadas
   y con `Orchestrator's Brain_0.0.1_x64_en-US.wxs` sin seguimiento. Se preservó.
   Tras `git fetch`, `origin/main` seguía en
   `a962e75f9f660b1a17daee4d71c1a0a1f11f69a0`, la base atribuida al MSI del
   ensayo. La corrección parte de ese commit en un worktree independiente,
   rama `codex/apertura-recuperacion-segura`; su destino es `main`. No incorpora
   cambios de `work` ni de otros worktrees.

2. **Causa confirmada.** SQLCipher autentica páginas, no la completitud de un
   respaldo. Una imagen antigua válida sin el WAL necesario puede tener
   `user_version=0` y cero objetos de esquema. `Store::open_with` la aceptaba,
   activaba WAL y ejecutaba `CREATE TABLE IF NOT EXISTS` y las migraciones;
   así convertía la pérdida de transacciones en una bóveda vacía aparentemente
   válida. En copias nuevas de las evidencias históricas se confirmó:

   | Copia sintética | Versión / objetos | Entidades / aserciones / fragmentos | Integridad |
   | --- | --- | --- | --- |
   | Completa con WAL | 2 / 39 | 15 / 156 / 8 | Correcta |
   | Sin WAL necesario | 0 / 0 | No hay tablas | También correcta |
   | Consolidada sin WAL | 2 / 39 | 15 / 156 / 8 | Correcta |

   Se ejecutaron `cipher_integrity_check`, `integrity_check` y
   `foreign_key_check` sobre copias desechables, nunca sobre el perfil
   funcional conservado.

3. **Decisión.** Separar creación explícita y reconocimiento de una base
   existente, sin cambiar el formato SQLCipher, el archivo de llave, la
   criptografía ni el DDL histórico. Se admiten versiones 1 y 2 con las
   columnas del log, tablas y triggers de protección históricos; v2 requiere
   además las tablas de publicación. Antes de migrar se comprueban estructura,
   `quick_check` y referencias. No se exige WAL: una base consolidada es válida.

   La inspección usa una carpeta temporal privada con copias del principal,
   WAL y journal, si existen. SHM se reconstruye allí. SQLite puede crear o
   modificar auxiliares incluso con una conexión de solo lectura; inspeccionar
   directamente la única copia no satisface la conservación pedida. La copia
   temporal se elimina por RAII y no se usa para sustituir el origen. Tras
   reconocerla, se abre el origen sin `CREATE` y se revalida antes del DDL.
   Esta inspección no constituye un respaldo consistente en caliente.
   Véanse las reglas de [SQLite sobre WAL y apertura de solo lectura](https://www.sqlite.org/wal.html#read_only_databases).

4. **Implementado.** `Store::create_with` reserva el archivo con `create_new`;
   `open` y `open_with` rechazan rutas con llave o auxiliares huérfanos y
   bases existentes vacías, desconocidas o de versión futura. La apertura del
   producto valida antes de eliminar restos de una conversión, cifrar o
   migrar. La recuperación descifra primero la caja sin escribir, comprueba
   la base y sólo entonces vuelve a envolver la llave para Windows. Una
   conversión pendiente requiere un origen Brain reconocible en claro y una
   copia cifrada reconocible; un `.cifrando` sobrante no sustituye una base ya
   cifrada. Los checkpoints deben terminar antes de mover el principal. La
   aplicación muestra un diálogo nativo con el error de apertura y el núcleo
   devuelve un mensaje que pide conservar los archivos y comprobar el respaldo.

5. **Regresiones reproducibles.** `core/obrain-core/tests/apertura.rs` reproduce
   la pérdida del WAL con SQLCipher real. El mismo archivo, copiado sin cambios
   al código original de `origin/main`, falla porque éste acepta la imagen sin
   esquema; en la corrección pasa. El bootstrap de esa regresión utiliza el
   DDL real y SQLCipher directamente para no depender de la nueva API de
   creación. El ensayo conserva el código original extraído de `origin/main`.
   `core/obrain-api/tests/apertura_segura.rs` ejercita el flujo del producto:
   datos sintéticos, WAL completo con y sin SHM, pérdida del WAL necesario,
   checkpoint, llaves ausente/incorrecta/huérfana, archivos vacíos o ajenos,
   versiones no admitidas, esquema incompleto, referencias rotas, recuperación,
   restos de conversión, migración v1 y dos conexiones. Se comparan hashes y
   nombres de archivos en los rechazos; las aperturas positivas comprueban
   recuentos 15/156/8, aserciones, FTS e integridad.

6. **Verificación ejecutada en Windows.** **267 tests pasan, cero fallos y cero
   ignorados**, incluyendo 19 regresiones del flujo del producto, la regresión
   del núcleo y las suites existentes de seguridad, migración v1, contenido,
   PIN, recuperación y procedencia de los tres paquetes modificados. Comandos
   desde `core`:

   ```powershell
   cargo test --locked -p obrain-api -p obrain-core -p obrain-keys
   cargo clippy --locked -p obrain-core -p obrain-keys --all-targets -- -D warnings
   cargo clippy --locked -p obrain-api --lib --bins --test apertura_segura -- -D warnings
   ```

   También pasan `cargo check --locked --manifest-path ui/src-tauri/Cargo.toml`,
   los formatos de ambos workspaces y `git diff --check`. Clippy global con
   `--workspace --all-targets -D warnings` se detuvo en una advertencia
   **preexistente** de `cloned_ref_to_slice_refs` en
   `core/obrain-api/tests/producto_windows.rs:144`; ese archivo no se modificó.
   Se completó Clippy de los paquetes y tests modificados por separado.
   El enlazador emitió avisos LNK4099 sobre los símbolos PDB de OpenSSL; las
   pruebas terminaron correctamente.

   En copias nuevas de las evidencias históricas, la versión corregida
   conserva 15 entidades, 156 aserciones y 8 fragmentos al abrir el conjunto
   completo y el consolidado sin WAL; sus tres comprobaciones de integridad
   pasan. La copia incompleta se rechaza por versión 0 y conserva exactamente
   **los nombres de archivos y los SHA-256 previos**. Las 12 entradas de
   conservación corresponden a los cuatro archivos del perfil funcional y
   sus ocho originales ficticios. Las evidencias del ensayo están en
   `D:\Ecosistema\ensayos\brain-apertura-segura-20261015\evidence`.
   El primer intento de compilación encontró un fallo de OpenSSL/Perl al
   generar rutas con el apóstrofo del checkout; se utilizó un `CARGO_TARGET_DIR`
   separado sin apóstrofos. No se cambió la configuración global de Windows.
   ONNX Runtime/DirectML se reutilizó de las dependencias nativas locales;
   no se descargaron modelos. Para comprobar Tauri se reutilizó también el
   OpenSSL ya compilado, mediante variables del proceso, evitando otra
   compilación completa. No se ejecutó un MSI ni se instaló o desinstaló Brain.
   El diálogo de error se comprobó por compilación, sin ensayo visual de una
   nueva compilación de escritorio. No se ha probado otro Windows físico ni
   se ha medido el rendimiento con bases grandes.

7. **Compatibilidad y límites de detección.** Se conservan las bases legítimas
   v1 y v2, tanto cifradas como en claro, incluidas las vacías con esquema
   reconocido y las consolidadas sin WAL. Una base existente sin esquema se
   rechaza aunque pudiera proceder de una creación interrumpida: no se puede
   distinguir ese estado de la pérdida demostrada. Una copia más antigua que
   ya tenga esquema y datos coherentes puede superar la validación aunque le
   falten transacciones posteriores. Tampoco se detecta la pérdida simultánea
   del principal, llave y todos sus restos: una ruta completamente nueva sigue
   siendo una creación legítima. Reconocer tablas y triggers no es una firma
   criptográfica de identidad ni una prueba de que el respaldo esté completo.

8. **Riesgos residuales.** La inspección añade lectura y espacio temporal
   proporcionales al principal y WAL, y comprobaciones de integridad; no se ha
   caracterizado su coste en bibliotecas grandes. Durante el cifrado de una
   base antigua en claro, su copia temporal también está en claro y una caída
   del proceso puede dejarla en TEMP. Una copia de archivos mientras otro
   proceso escribe puede ser incoherente y causar un rechazo conservador;
   hay que cerrar las otras instancias y reintentar. La revalidación evita
   confiar únicamente en esa copia, pero no proporciona atomicidad frente a
   sustituciones externas concurrentes de archivos ni garantiza hashes
   invariantes si el origen cambia durante la apertura. Los checkpoints
   comprobados no sustituyen un protocolo de exclusión entre procesos para
   cifrar y renombrar. El PIN no prueba completitud y la recuperación en otro
   equipo físico sigue sin ensayarse.

9. **PR.** Pendiente de publicación; no hacer merge ni actualizar la instalación.
   El commit utiliza `[skip ci]` para omitir el workflow automático de MSI,
   que instala y desinstala Brain. Las comprobaciones descritas son locales;
   el ensayo del instalador queda pendiente. Es el mecanismo de
   [omisión de workflows de GitHub](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).

10. **Siguiente paso propuesto, no implementado.** Un flujo propio de respaldo
    consistente con SQLite Backup API o exportación controlada, acompañado de
    un manifiesto externo con identidad, versión, hashes y recuentos del
    conocimiento, y una restauración de ensayo aislada que compruebe datos y
    acceso criptográfico. El manifiesto debe conservarse junto al conjunto y
    el código de recuperación fuera del equipo. Es una segunda iteración,
    pendiente de decisión; esta corrección no la inicia.
