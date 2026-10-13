# Validación Windows de v1 → v2 antes de usar datos personales

Fecha: 13 de octubre de 2026. Estado de partida: PR #4 fusionada en `main`,
commit `8e5496f09277180c8a033ddab5e5188d4030d8c6`. Todo lo descrito aquí usa
bases y archivos sintéticos. La bóveda personal, sus llaves y sus respaldos no
se abrieron, copiaron ni migraron.

## Fixture y aislamiento

`examples/fixture_v1_windows.rs` reutiliza el DDL v1 conservado en
`tests/fixtures/schema_v1.sql` (tomado de `683e63b`) y el constructor de
`tests/fixtures/legacy_v1.rs`. Añade los `folder_id` y `file_id` de los
archivos de prueba, presentes tras un escaneo v1 real. Contiene:

| Documento | Estado heredado | Resultado requerido |
| --- | --- | --- |
| `verificado.txt` | Hash del archivo y de la extracción coinciden; fecha inferida antigua | Publicación `ready`, consulta `faro`, vista previa y retirada de la fecha obsoleta |
| `cuarentena.pdf` | Fragmento heredado con hash distinto; archivo PDF inválido | Publicación `pending`, sin resultado `secreto-sintetico` ni vista previa textual |

La fixture se crea bajo una raíz temporal exclusiva. Antes de iniciar Tauri,
`tools/validar-v1-windows.ps1` comprueba `user_version=1`, la ausencia de la
tabla v2 y que **todas** las rutas registradas de biblioteca/documento caigan
dentro de esa raíz. El lanzador fija `OBRAIN_DB`, `USERPROFILE`, `HOME`,
`LOCALAPPDATA`, `APPDATA`, `TEMP`, `TMP`, `XDG_*` y
`WEBVIEW2_USER_DATA_FOLDER` a subdirectorios de prueba.

La revisión del código confirmó las entradas concretas: `default_db_path()`
respeta `OBRAIN_DB` antes de consultar la ubicación heredada;
`obrain_core::ubicacion::modelos()` toma los perfiles fijados; la llave se
guarda junto a la base; el indexador de fondo vuelve a llamar
`default_db_path()` y sólo reindexa las bibliotecas registradas en esa base.
El OCR recibe bytes y no rutas. El selector de carpetas no se usa en esta
validación. WebView2 documenta que `WEBVIEW2_USER_DATA_FOLDER` sustituye el
perfil del navegador: <https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl>.

## Caída nativa y corrección

En el estado de `main`, la prueba de dos bases v1 desde hilos sucesivos del
mismo proceso reprodujo `STATUS_ACCESS_VIOLATION` (`0xc0000005`). La primera
base completó migración, segunda conexión y reapertura; la segunda cayó al
inicializar la publicación heredada. `gdb` detuvo el hilo en `SIGSEGV` y el
PDB simbolizado situó la pila en:

```text
windows::Media::Ocr::OcrEngine::TryCreateFromUserProfileLanguages
windows_core::imp::factory_cache::FactoryCache::call
obrain_ocr::windows_ocr::WindowsOcr::create
obrain_ocr::platform
obrain_api::publication::bootstrap_legacy (publication.rs:119)
Brain::initialize_store → Brain::try_from_store
```

`RoInitialize` había devuelto éxito en ambos hilos. La factoría generada de
OCR quedaba en una caché global y la llamada posterior desde otro hilo caía.
La rama de corrección obtiene la factoría OCR para cada llamada y equilibra
cada inicialización WinRT exitosa con `RoUninitialize` en el mismo hilo. La
prueba original ahora termina con las dos bases migradas y reabiertas en el
mismo proceso. Microsoft exige equilibrar cada `RoInitialize` exitoso:
<https://learn.microsoft.com/windows/win32/api/roapi/nf-roapi-roinitialize>.

También se reemplazaron las llamadas de producción de la CLI a
`Brain::from_store` por la variante fallible. `with_embedder` también propaga
el error. Dentro de `src/`, la única llamada restante a `from_store` está en
un módulo de pruebas en memoria; las demás llamadas del repositorio están en
tests.

## Recorridos ejecutados

| Recorrido | Resultado | Evidencia / límite |
| --- | --- | --- |
| DDL v1, dos migraciones desde hilos sucesivos | Falló en `main`; pasó con la corrección | Traza nativa anterior; `recuperacion_v1_hilos_windows` |
| Segunda conexión y reapertura de ambas bases | Pasó con la corrección | Misma prueba, `quick_check=ok`, visibilidad y cuarentena |
| Frontend `npm run build` | Pasó | 0 errores y 0 advertencias |
| Frontend `npm test` | Pasó | 12 pruebas |
| Núcleo `cargo test --locked --workspace` | Pasó | Suite completa con perfil de prueba; ninguna prueba fallida |
| OCR en tres hilos sucesivos, rutas imagen/PDF | Pasó | Reconocedor `windows.media.ocr/es-MX` disponible; bytes sintéticos inválidos devolvieron error sin caída |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pasó | Perfil y caché ORT temporales |
| Tauri debug, arranque, hilo de fondo, reapertura | Pasó | Proceso activo, llave y perfil WebView2 sólo en la raíz temporal; inspector tras ambos arranques |
| Tauri release, arranque, hilo de fondo, reapertura | Pasó | Binario de `tauri build --ci --no-sign --bundles nsis`, fixture v1 nueva, inspector tras ambos arranques |
| MSI y su carga extraída, migración y reapertura | Pasó | `tauri bundle --ci --no-sign --bundles msi`; `dark.exe -x` bajo TEMP; SHA-256 de la carga igual al binario release; tercera fixture v1 |
| Instalación MSI / NSIS | No ejecutada | No se instaló nada sobre el perfil real; NSIS no pudo generar instalador por error de macro de `makensis` |

La primera fixture omitía los FileID y el indexador aplazó correctamente esas
rutas por ambigüedad de identidad. Se descartó esa ejecución para el resultado
de consultas y se generó una nueva fixture v1 con FileID. En la segunda,
`integrity_check=ok`, `foreign_key_check=0`, `user_version=2`, documento
verificado `ready`, búsqueda y vista previa válidas; el PDF inválido quedó
`pending`, con extracción `failed` actualizada por el hilo de fondo, sin
resultado ni vista previa de su texto. La reapertura repitió esos controles.
El arranque se comprobó mediante proceso Tauri vivo, creación del perfil
WebView2 y observación independiente de la base; no se automatizaron clics en
la ventana. La suite Rust completa se ejecutó con `USERPROFILE`,
`LOCALAPPDATA`, `APPDATA`, `TEMP` y `TMP` de prueba, conservando sólo
`RUSTUP_HOME` y `CARGO_HOME` del usuario para encontrar el compilador.

El inspector pasó también en release y en la carga extraída del MSI: una
biblioteca, `integrity_check=ok`, cero fallos de claves foráneas, publicación
`ready/index` del texto verificado y `pending/legacy` del contenido en
cuarentena, búsqueda y vista previa sólo del verificado. La extracción fallida
del PDF se actualizó desde el hilo de fondo en cada arranque y reapertura. El
MSI se generó después de declarar el icono `.ico` que ya existía en el
repositorio. El ejecutable extraído coincidió byte a byte con el binario
release (`SHA-256
FF3369B3F94E19041DF997B485C20DCAF678BF085547E2D9804D06420321C06D`).
El MSI de prueba no está firmado y su instalación real queda sin probar. El
empaquetado NSIS falló con
`IsShortcutTarget: macro NSISCOMCALL requires 4 parameter(s), passed 7`;
el script generado interpola `PRODUCTNAME` (que contiene un apóstrofo) en la
macro de acceso directo de `utils.nsh`. Eso no impidió compilar ni ejecutar el
binario release. Hace falta resolver NSIS si ése es el formato de distribución
elegido.

## Reproducción aislada en Windows

Desde la raíz del repositorio, con Python 3, Rust y las dependencias de Tauri:

```powershell
$root = Join-Path $env:TEMP ('obrain-v1-' + [guid]::NewGuid().ToString('N'))
$temp = Join-Path $root 'temp'
New-Item -ItemType Directory -Force $temp | Out-Null
$env:TEMP = $temp
$env:TMP = $temp
$env:CARGO_TARGET_DIR = Join-Path $root 'target'
Remove-Item Env:LC_ALL,Env:LC_CTYPE,Env:LANG -ErrorAction SilentlyContinue
cargo build --manifest-path core/Cargo.toml -p obrain-api --example fixture_v1_windows --example inspect_v1_windows
$db = & (Join-Path $env:CARGO_TARGET_DIR 'debug/examples/fixture_v1_windows.exe')
$exe = Join-Path $env:CARGO_TARGET_DIR 'debug/obrain-app.exe'
$inspector = Join-Path $env:CARGO_TARGET_DIR 'debug/examples/inspect_v1_windows.exe'
```

El ejecutable Tauri se compila con
`cargo build --manifest-path ui/src-tauri/Cargo.toml`, manteniendo
`CARGO_TARGET_DIR` en una ruta sin apóstrofo (OpenSSL falla al configurar con
la ruta de este repositorio). Para debug, iniciar `npm run dev -- --host
127.0.0.1` desde `ui` en otra consola y mantener Vite activo. Antes de iniciar
la aplicación:

```powershell
.\tools\validar-v1-windows.ps1 -Root $root -Database $db -Executable $exe -PreflightOnly
```

Iniciar la aplicación con el mismo comando sin `-PreflightOnly`, esperar al
indexado y ejecutar `& $inspector $db` desde otra consola, con **la ruta
explícita de la fixture**. Cerrar, volver a lanzar con `-AlreadyMigrated` y
repetir el inspector. Para la versión release, crear otra fixture v1 desde cero
y repetir el recorrido. `npm run tauri -- build --ci --no-sign --bundles nsis`
compila el binario release pero el empaquetado NSIS falla en este entorno. Con
ese binario, `npm run tauri -- bundle --ci --no-sign --bundles msi` genera el
MSI. `dark.exe -x` de WiX permite extraer la carga bajo la raíz temporal y
ejecutar `obrain-app.exe` junto a `DirectML.dll` sin instalarlo. Confirmar el
SHA-256 del ejecutable extraído y repetir el preflight y el inspector con una
fixture nueva. No usar el instalador sobre el perfil personal.

## Procedimiento para una futura primera migración personal

Este procedimiento es un plan para una sesión posterior expresamente
autorizada. No se ha ejecutado. Antes de empezar, la corrección debe estar
integrada y deben haber pasado los recorridos Tauri de desarrollo y release.

1. Identificar la generación activa y su ubicación **sin abrirla**: base,
   `<base>.llave`, `-wal`, `-shm` y cualquier `.cifrando`. Resolver la posible
   ubicación anterior de `%LOCALAPPDATA%` frente a `%USERPROFILE%\.obrain`;
   si hay dos generaciones, detenerse hasta decidir cuál es la activa. No
   registrar nombres de archivos personales ni claves en el informe.
2. Detener aplicación, CLI, indexador y cualquier otro escritor. Confirmar que
   no quedan procesos ni manejadores abiertos. Si no puede probarse la
   quietud, usar una copia consistente del motor SQLCipher; **nunca** copiar
   sólo el archivo principal con WAL activo. Mantener la aplicación cerrada
   hasta terminar el respaldo.
3. Copiar offline toda la familia de archivos de la generación activa a un
   directorio protegido, con permisos sólo del propietario. Conservar dos
   copias: una inmutable para vuelta atrás y otra de trabajo. Comparar lista,
   tamaños y SHA-256 de cada archivo del origen y la copia antes de abrir
   cualquiera. Registrar únicamente conteos y hashes en un manifiesto
   privado. Guardar el material de recuperación por separado; no imprimirlo.
4. Restaurar la copia de trabajo en una ruta aislada bajo **el mismo usuario de
   Windows** (DPAPI). Confirmar de nuevo los hashes antes de abrirla. Abrir
   sólo esa restauración con un comprobador de mantenimiento que no reindexe ni
   siga las bibliotecas personales. Verificar desbloqueo o recuperación,
   `integrity_check=ok`, `foreign_key_check=0`, versión, conteos de entidades
   y aserciones, estados de publicación, correcciones del usuario, búsquedas y
   vistas autorizadas. Si el comprobador aún no existe o algo falla, no
   avanzar.
5. Repetir la migración de ensayo sobre otra restauración, conservar la copia
   inmutable y contrastar los controles. Preparar el mismo manifiesto de
   comprobaciones para la generación activa. Sólo entonces proponer una
   ventana de migración personal al usuario; ninguna prueba sintética la
   autoriza por sí sola.
6. Tras una migración autorizada, comprobar integridad, consultas,
   correcciones, cuarentena y estado de publicación antes de permitir
   reindexación. Si falla, cerrar todos los procesos, apartar la generación
   fallida sin borrarla y restaurar el conjunto **completo** de base, WAL y
   llave desde la copia inmutable. Verificar hashes y reapertura. No mezclar
   archivos de generaciones distintas ni borrar el respaldo durante la
   validación.

La primera migración personal permanece bloqueada hasta integrar esta
corrección, verificar una restauración de respaldo de la generación personal
en una sesión posterior autorizada y comprobar la instalación aislada del
formato de distribución elegido. Si se elige NSIS, antes hay que resolver su
error de empaquetado. Ninguno de esos pasos autoriza por sí solo la migración
personal: se requiere autorización expresa para esa sesión futura.
