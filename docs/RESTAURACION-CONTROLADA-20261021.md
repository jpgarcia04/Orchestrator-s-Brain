# Restauración real, controlada y reversible de perfiles cifrados

Fecha: 21 de octubre de 2026  
Rama: `codex/restauracion-controlada`  
Estado: Implementado y verificado con 20 pruebas de integración sintéticas

---

## 1. Diseño elegido y justificación

Los perfiles de conocimiento de Orchestrator's Brain residen en un conjunto acoplado de archivos cifrados mediante SQLCipher 4 (`brain.obrain`, `brain.obrain.llave`, y en tiempo de ejecución sus diarios WAL `-wal` y `-shm`).

Un mecanismo de restauración ingenuo (como un simple copiado o sobrescritura de archivos) presentaba graves riesgos para la integridad del conocimiento:

1. **No atomicidad multiarchivo en Windows (NTFS)**: Windows no ofrece operaciones atómicas nativas que sustituyan simultáneamente la base de datos y su material criptográfico (.llave). Si una operación se interrumpe tras sustituir un archivo pero antes del otro, el perfil queda inutilizable o corrupto (llave desparejada con los datos).
2. **Violaciones de compartición (`ERROR_SHARING_VIOLATION`)**: En Windows, un archivo abierto por SQLite no puede eliminarse ni renombrarse mientras existan descriptores activos en cualquier proceso (interfaz Tauri, indexador en segundo plano, vigilante de bibliotecas o terminales CLI).
3. **Pérdida irreversible ante respaldos antiguos o defectuosos**: Si se sustituye el perfil activo directamente y el respaldo restaurado está dañado, incompleto o pertenece a un estado obsoleto, el conocimiento previo del usuario se perdería irremediablemente.
4. **Desfase de claves entre equipos (DPAPI)**: Una base respaldada en otra máquina o usuario no puede descifrar su caja del sistema localmente; requiere re-empaquetar el material criptográfico con la clave de recuperación antes de que la base pueda operar en el equipo receptor.

### Principio rector de la arquitectura implementada

Se diseñó un protocolo transaccional en siete etapas estrictamente acotadas, complementado con un esquema de autorización explícita en dos fases:

```
[Respaldo] ──(1. Verificar en solo lectura)──> [Plan de sustitución / Diagnóstico]
                                                            │
                                             (Exige autorización: --confirmar)
                                                            ▼
[Perfil activo] ──(2. Comprobar descriptores / lock)──> [Garantía de base no en uso]
      │
      ├──(3. Salvaguarda sincronizada)──> [salvaguardas/salvaguarda-<ts>-<hex>/]
      │
[Staging aislado] ──(4. Re-envolver DPAPI + pausar sync)──> [.obrain-restaurar-staging-*/]
      │
[Recibo persistente] ──(5. Escribir recibo)──> [brain.obrain.transicion.json]
      │
[Sustitución atómica] ──(6. Desplazar activos a .anterior + renombrar staging)
      │
[Validación final] ──(7. Brain::open + verificación huella)
      │
      ├── Éxito: Limpieza de .anterior, staging y recibo.
      └── Fallo: Rollback automático desde .anterior / salvaguarda.
```

---

## 2. Procedimiento de restauración y salvaguarda

### Fases operativas en el núcleo (`obrain_api::respaldo`)

1. **Detección y curación previa (`comprobar_y_reparar_transicion`)**:  
   Antes de iniciar cualquier acción, comprueba si existe un recibo de transición pendiente (`brain.obrain.transicion.json`) derivado de una caída eléctrica anterior. Si lo detecta, recupera automáticamente el perfil original antes de proceder.

2. **Verificación exhaustiva del respaldo (`verificar_con_clave`)**:  
   En modo estrictamente solo lectura:
   - Comprueba integridad de tamaño y hash BLAKE3 de cada archivo del respaldo frente al manifiesto.
   - Si la llave requiere clave de recuperación por provenir de otro equipo, valida la clave mediante Argon2id + XChaCha20-Poly1305.
   - Valida la integridad de páginas autenticadas (`PRAGMA cipher_integrity_check`) y la consistencia estructural de B-trees (`PRAGMA integrity_check`).
   - Verifica la huella de contenido canónica.

3. **Comprobación de exclusividad de conexión (`comprobar_conexiones_activas`)**:  
   - Ejecuta un `PRAGMA wal_checkpoint(TRUNCATE)` con timeout para asegurar que no haya transacciones pendientes de consolidar.
   - En Windows, abre el archivo con `share_mode(0)` (exclusividad a nivel de SO) tanto sobre `brain.obrain` como sobre `brain.obrain-wal`. Si cualquier proceso (Tauri, vigilante, CLI) mantiene la base abierta, la operación aborta limpiamente devolviendo `Error::BaseEnUso` sin alterar ningún archivo.

4. **Creación de salvaguarda completa y verificable (`crear_salvaguarda`)**:  
   Si el perfil activo existe, antes de tocarlo:
   - Crea un directorio aislado `salvaguardas/salvaguarda-<ts>-<hex>/`.
   - Copia de forma sincronizada a disco (`sync_all`) la base activa, su `.llave` y sus archivos auxiliares WAL/SHM (si tienen contenido).
   - Genera un archivo `info.json` con los metadatos de la base sustituida, marcas temporales y hashes BLAKE3 para auditoría o reversión manual.

5. **Preparación en *staging* aislado y re-empaquetado criptográfico**:  
   - En un directorio privado en el mismo volumen (`.obrain-restaurar-staging-*`):
   - Copia la base y la llave del respaldo.
   - Si la base procede de otro equipo, ejecuta `seguridad::recuperar` con el código de recuperación proporcionado, re-envolviendo la llave con el DPAPI de la sesión actual sin tocar el respaldo original.
   - Si la opción `pausar_reindexado` está activa (por defecto), registra en la tabla `ajustes` la suspensión de reindexación y ejecuta checkpoint.

6. **Recibo de transición persistente (`ReciboTransicion`)**:  
   - Escribe y sincroniza a disco `brain.obrain.transicion.json` en la carpeta padre de la base activa.
   - Registra hashes BLAKE3 de la versión preparada, hashes anteriores de la base activa, ruta de salvaguarda y estado (`"preparado"` → `"reemplazando"`).

7. **Sustitución controlada por desplazamiento**:  
   - Renombra `brain.obrain` a `brain.obrain.anterior` y `brain.obrain.llave` a `.llave.anterior`.
   - Elimina los archivos temporales `-wal`, `-shm` y `-journal` del perfil anterior.
   - Renombra atómicamente los archivos preparados desde staging hacia las rutas de destino activas.

8. **Validación post-sustitución y limpieza**:  
   - Abre la base recién colocada en destino mediante `Brain::open(&db_destino)`.
   - Comprueba que la huella de contenido, el número de entidades y las aserciones coincidan exactamente con el manifiesto verificado.
   - Al validar el éxito: elimina los archivos `.anterior`, suprime el recibo de transición y borra la carpeta de staging.

---

## 3. Comportamiento ante errores e interrupciones

El sistema implementa dos capas complementarias de tolerancia a fallos:

### Rollback automático en memoria (errores durante la ejecución)
Si se produce un error de E/S, fallo de validación o excepción en cualquier punto del proceso:
- La función `revertir_sustitucion` elimina de inmediato cualquier archivo incompleto en las rutas de destino.
- Restaura los archivos originales desde `.anterior` o, si estos no estuvieran disponibles, desde la carpeta de salvaguarda recién creada.
- Limpia los archivos `.anterior` y el recibo de transición.
- Devuelve un error explícito `Error::RestauracionFallidaYRecuperada`, garantizando que la base activa previa vuelve a abrirse con el 100% de su conocimiento intacto.
- Si incluso la restauración desde la salvaguarda encontrara un error crítico del sistema de archivos, devuelve `Error::RestauracionCritica` indicando la ubicación exacta de la salvaguarda para recuperación manual.

### Recuperación ante corte eléctrico o terminación abrupta (Crash Recovery)
Si el equipo sufre un corte de energía o el proceso es terminado forzosamente (`SIGKILL` / `taskkill`) a mitad del reemplazo:
- El recibo `brain.obrain.transicion.json` permanece en el disco.
- Conectado directamente al recorrido de apertura (`seguridad::abrir` y `Brain::open`): en el siguiente arranque de la aplicación o ejecución de la CLI, la verificación se ejecuta automáticamente antes de tocar el almacén.
- **Detección fiable y preservación ante ambigüedad**:
  - Si la sustitución completó satisfactoriamente (hashes en destino coinciden con el staging), limpia los temporales y el recibo.
  - Si la base activa no llegó a desplazarse (intacta con hash previo), limpia el staging y el recibo sin tocar la base activa.
  - Si los archivos fueron desplazados a `.anterior` o están en la salvaguarda y sus hashes coinciden con los registrados, revierte de forma determinista restableciendo el estado previo.
  - **Ante ambigüedad** (archivos destino con datos no reconocidos, hashes discrepantes o ausencia de fuente fiable de reversión), el sistema **se detiene de inmediato** arrojando `Error::TransicionAmbigua` y **preserva todas las evidencias en disco** (recibo, temporales, bases previas) sin realizar modificaciones destructivas.

---

## 4. Tratamiento de conexiones activas

El mayor riesgo de corrupción o fallo en caliente radica en la presencia de conexiones concurrentes en SQLite.

- **Comprobación en dos niveles**:
  1. *Nivel SQLite*: Comprueba mediante `PRAGMA wal_checkpoint(TRUNCATE)` con timeout que no existan lectores activos reteniendo frames en el WAL (`busy != 0`).
  2. *Nivel Sistema Operativo*: En Windows, invoca `std::os::windows::fs::OpenOptionsExt::share_mode(0)` sobre `brain.obrain` y `brain.obrain-wal`. Esto solicita acceso sin compartición. Dado que SQLite abre sus archivos con `FILE_SHARE_READ | FILE_SHARE_WRITE`, si cualquier otro hilo o proceso tiene la base abierta, la llamada falla de inmediato con `PermissionDenied` (`ERROR_SHARING_VIOLATION`).
- **Comprobación repetida inmediatamente antes de la sustitución**:
  - Las comprobaciones de exclusividad se repiten justo antes de mover archivos a `.anterior` (paso 6), descartando conexiones que se hayan abierto durante la fase de staging o derivación criptográfica.
  - Se re-verifica que la huella BLAKE3 de la base activa no haya cambiado desde el diagnóstico inicial; si varió, la operación aborta limpiamente para proteger los cambios concurrentes.
- **Comportamiento seguro**:
  - Al detectar descriptores en uso, la operación aborta inmediatamente con `Error::BaseEnUso(ruta)`.
  - No se renombra ningún archivo activo ni se deja ninguna carpeta de staging abandonada.

---

## 5. Tratamiento de bibliotecas cuyos originales no están disponibles

Uno de los mayores peligros al restaurar un respaldo en otro equipo es el comportamiento del indexador automático:

- Si un respaldo contiene bibliotecas indexadas en rutas que no existen en el equipo destino (e.g. `D:\Empresa\Contratos`):
  - El escáner de Orchestrator's Brain comprueba `!root.is_dir()`, añade la biblioteca a `missing` y **aborta el recorrido sin mandar ningún archivo a la papelera**. El conocimiento indexado (texto, vectores, entidades, relaciones) permanece intacto.
- Sin embargo, si la ruta *sí existe* en el equipo local pero tiene archivos diferentes, una reindexación automática descontrolada mandaría a la papelera de 30 días los documentos ausentes.

### Solución: Reindexación pausada post-restauración

1. **Suspensión preventiva**:  
   `restaurar` fija de forma predeterminada el ajuste `seguridad::pausar_reindexado` con el motivo `"restauración de respaldo reciente"`.
2. **Protección en el escáner**:  
   Al arrancar, `indexar::index` e `indexar::reindex` comprueban si la reindexación está suspendida. Si lo está, abortan de inmediato informando de la suspensión:
   ```text
   reindexación suspendida (restauración de respaldo reciente): revisa tus bibliotecas y confírmalas con 'obrain bibliotecas --confirmar' antes de actualizar
   ```
3. **Flujo de confirmación del usuario**:
   - `obrain restaurar` informa detalladamente qué bibliotecas están presentes y cuáles faltan en este equipo.
   - `obrain bibliotecas` muestra un aviso destacado indicando que la reindexación está en pausa.
   - Cuando el usuario ha revisado o reubicado sus carpetas, ejecuta:
     ```powershell
     obrain bibliotecas --confirmar
     ```
     lo que elimina la suspensión y reanuda la sincronización normal.

---

## 6. Resultados de pruebas

Todas las pruebas se ejecutaron sobre bases sintéticas desechables en carpetas temporales con identificadores aleatorios, sin tocar perfiles de usuario ni archivos de producción.

### Resumen de ejecución

- **Suite de respaldos y restauración (`core/obrain-api/tests/respaldos.rs`)**:  
  **20 pruebas ejecutadas, 20 aprobadas (100%), 0 fallos**.
- **Suite completa del workspace (`cargo test --manifest-path core/Cargo.toml`)**:  
  **289+ pruebas aprobadas en todos los módulos (`obrain-keys`, `obrain-core`, `obrain-scan`, `obrain-extract`, `obrain-expiry`, `obrain-guard`, `obrain-link`, `obrain-embed`, `obrain-api`)**.
- **Linters y formato**:
  - `cargo clippy --all-targets -- -D warnings`: Aprobado (0 advertencias).
  - `cargo fmt --check`: Aprobado (0 diferencias).

### Matriz de escenarios de restauración verificados

| Prueba | Escenario verificado | Resultado |
|---|---|---|
| `restauracion_correcta_de_base_cifrada_con_salvaguarda_verificable` | Restauración íntegra de base realista; verificación de salvaguarda abrible con datos previos; conservación de PIN, colecciones, documentos en bóveda protegidos, carpetas sensibles, caducidades y búsqueda FTS5. | APROBADO |
| `restauracion_requiere_autorizacion_y_respeta_plan` | `planificar` genera diagnóstico completo; restaurar sin `--confirmar` falla con `NoAutorizado`; base activa permanece 100% inalterada. | APROBADO |
| `restauracion_rechaza_conexiones_abiertas` | Conexión activa retenida sobre la base; intento de restauración falla con `BaseEnUso`; tras liberar la conexión, la restauración concluye con éxito. | APROBADO |
| `restauracion_falla_con_respaldo_invalido_sin_tocar_perfil_activo` | Bytes alterados en el archivo del respaldo; fallo en verificación antes de tocar archivos activos; perfil previo intacto. | APROBADO |
| `restauracion_con_clave_en_entorno_dpapi_distinto` | Simulación de máquina ajena con DPAPI incompatible; planificador exige clave; restauración re-empaqueta con código de recuperación; base activa abre nativamente con DPAPI local. | APROBADO |
| `restauracion_rollback_ante_fallos_en_distintas_etapas` | Inyección de errores programados tras crear salvaguarda, tras desplazar activos a `.anterior` y antes de validar; en todos los casos el rollback restaura la base activa original sin pérdida. | APROBADO |
| `restauracion_repara_transicion_interrumpida` | Simulación de corte eléctrico en pleno desplazamiento con recibo de transición; `comprobar_y_reparar_transicion` recupera deterministamente el perfil original. | APROBADO |
| `apertura_repara_transicion_interrumpida_directamente` | Simulación de interrupción; llamada directa a `Brain::open` repara y abre la base automáticamente sin intervención manual. | APROBADO |
| `restauracion_transicion_ambigua_se_detiene_y_preserva_evidencias` | Recibo con estado no reconocible fiablemente; la apertura y curación se detienen con `TransicionAmbigua` sin tocar ni borrar evidencias. | APROBADO |
| `restauracion_de_copia_antigua_frente_a_mas_reciente` | Base activa con más entidades que el respaldo; el planificador advierte explícitamente; con confirmación se restaura y la salvaguarda conserva el estado más reciente. | APROBADO |
| `restauracion_pausa_reindexado_y_bibliotecas_ausentes` | Restauración suspende reindexación; intentos de reindexar abortan con aviso descriptivo; reanudación explícita con `reanudar_reindexado` no purga documentos ausentes. | APROBADO |
| `cli_obrain_restaurar_plan_y_ejecucion_con_confirmacion` | Pruebas de extremo a extremo mediante el binario CLI `obrain`: diagnóstico sin `--confirmar`, ejecución con `--confirmar`, consulta de bibliotecas y reanudación con `obrain bibliotecas --confirmar`. | APROBADO |

---

## 7. Riesgos residuales y límites reales de la reversibilidad

### Demostrado mediante pruebas automatizadas
- La sustitución no destruye el estado anterior si se interrumpe durante el staging, el desplazamiento a `.anterior` o la validación final.
- El rollback restablece los archivos originales idénticos bit a bit.
- La desconexión abrupta se repara de forma determinista mediante el recibo de transición.
- Los secretos (clave de recuperación, PIN) no se exponen en argumentos de CLI ni en registros de error.

### Garantías inferidas y límites del entorno
- **Límites de NTFS / Sistema de archivos**: Windows no ofrece transacciones multiarchivo atómicas a nivel de kernel. El protocolo implementado mitiga este límite mediante el orden estricto de renombrados y el recibo persistente sincronizado con `sync_all`.
- **Espacio en disco**: Durante la operación se requiere espacio temporal para la salvaguarda y el staging (aproximadamente el doble del tamaño de la base). Si el disco se llena durante la fase de salvaguarda, la operación aborta limpiamente antes de alterar los archivos activos.
- **Archivos externos**: El respaldo conserva la totalidad del cerebro (texto extraído, índices, vectores, relaciones, colecciones y notas). No incluye ni puede regenerar los archivos físicos originales (PDF, Word, imágenes) si el usuario los ha borrado de su disco.

---

## 8. Recomendación para la futura integración en Tauri

Para integrar este mecanismo en la interfaz gráfica de usuario en una iteración posterior:

1. **Gestión del ciclo de vida de la conexión**:
   - Tauri mantiene abierto un handle persistente de `Brain` y un hilo con `obrain_api::vigilar`.
   - Antes de iniciar la restauración sobre la base activa, el backend de Tauri debe:
     1. Detener el vigía de bibliotecas (`vigilar::stop`).
     2. Liberar y destruir la instancia activa de `Brain` (para cerrar los file handles de SQLite en Windows).
2. **Flujo de usuario en dos pasos**:
   - **Paso 1 (Diagnóstico)**: Al seleccionar un archivo de respaldo, llamar a `obrain_api::respaldo::planificar`. Presentar una tarjeta de previsualización que muestre:
     - Fecha y número de entidades del respaldo vs perfil actual.
     - Advertencia destacada si el perfil actual contiene más aserciones o novedades.
     - Estado de las bibliotecas (indicando cuántas carpetas coinciden y cuáles no existen en esta máquina).
     - Campo protegido para introducir el código de recuperación sólo si el plan indica `requiere_clave_recuperacion`.
   - **Paso 2 (Ejecución)**: Botón modal «Confirmar sustitución». Al completarse:
     - Reabrir `Brain::open`.
     - Mostrar un banner en la vista principal: *«Perfil restaurado. La sincronización de bibliotecas está en pausa para proteger tu conocimiento. [Revisar bibliotecas] [Reanudar sincronización]»*.
