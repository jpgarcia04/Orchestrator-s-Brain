# Verificar el PIN en Windows con datos sintéticos

No usar una instancia personal ni `npm run app` sin aislamiento. El arranque
indexa las bibliotecas registradas. Estas instrucciones crean una base nueva,
su llave y una biblioteca bajo un directorio temporal único. No copiar una
bóveda, una llave ni un perfil WebView existente.

## Preparar y arrancar

Desde la raíz del repositorio, PowerShell. El ejecutable nativo incorpora el
frontend construido; `--no-bundle` evita instalar MSI/NSIS. No prueba el
instalador ni el perfil release.

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE 'obrain-target'
cargo build --locked --manifest-path core/Cargo.toml -p obrain-api --bin obrain
if ($LASTEXITCODE -ne 0) { throw 'Fallo al construir CLI' }
Push-Location ui
try {
    npm run tauri -- build --debug --no-bundle
    if ($LASTEXITCODE -ne 0) { throw 'Fallo al construir app' }
} finally { Pop-Location }

$taskRoot = Join-Path ([IO.Path]::GetTempPath()) ('obrain-pin-aislado-' + [guid]::NewGuid().ToString('N'))
$taskLibrary = Join-Path $taskRoot 'Biblioteca/Prueba'
$taskDb = Join-Path $taskRoot 'brain.obrain'
New-Item -ItemType Directory -Path $taskLibrary | Out-Null
Set-Content -LiteralPath (Join-Path $taskRoot 'ISOLATED-FIXTURE.txt') -Value 'Sólo datos sintéticos'
Set-Content -LiteralPath (Join-Path $taskLibrary 'documento.txt') -Value "MARCADOR_PIN_SINTETICO. Documento de prueba.`nVence el 1 de diciembre de 2026."
Add-Type -AssemblyName System.Drawing
$taskImage = [Drawing.Bitmap]::new(80,60)
$taskGraphics = [Drawing.Graphics]::FromImage($taskImage)
$taskGraphics.Clear([Drawing.Color]::Coral)
$taskImage.Save((Join-Path $taskLibrary 'imagen.png'), [Drawing.Imaging.ImageFormat]::Png)
$taskGraphics.Dispose()
$taskImage.Dispose()
$taskCli = Join-Path $env:CARGO_TARGET_DIR 'debug/obrain.exe'
# Suprimir también la clave de recuperación sintética; no registrarla.
& $taskCli indexar (Join-Path $taskRoot 'Biblioteca') --db $taskDb --sin-modelo *> $null
if ($LASTEXITCODE -ne 0) { throw 'Fallo al indexar fixture' }
& $taskCli sensible $taskLibrary --db $taskDb *> $null
if ($LASTEXITCODE -ne 0) { throw 'Fallo al marcar fixture' }
& $taskCli clave --guardada --db $taskDb *> $null
if ($LASTEXITCODE -ne 0) { throw 'Fallo al confirmar clave sintética' }

$taskStart = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:CARGO_TARGET_DIR 'debug/obrain-app.exe'))
$taskStart.UseShellExecute = $false
$taskStart.CreateNoWindow = $true
$taskStart.WorkingDirectory = $taskRoot
$taskStart.Environment['OBRAIN_DB'] = $taskDb
$taskStart.Environment['USERPROFILE'] = Join-Path $taskRoot 'Perfil'
$taskStart.Environment['LOCALAPPDATA'] = Join-Path $taskRoot 'AppData'
$taskStart.Environment['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $taskRoot 'WebView'
$taskApp = [Diagnostics.Process]::Start($taskStart)
```

Las variables de perfil se cambian sólo para el proceso hijo: la resolución
de modelos y de ubicaciones antiguas queda dentro del fixture. `OBRAIN_DB`
selecciona la base del frontend y del indexador. WebView2 admite el
[override del directorio de datos](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl#CreateCoreWebView2EnvironmentWithOptions),
para aislar localStorage y cache. Verificar que `WebView` se crea dentro del
fixture y que el arranque sólo muestra dos documentos sintéticos; detener la
prueba si aparecen bibliotecas, documentos o clave de otra instancia.

## Comprobación manual

1. En Seguridad, poner un **PIN exclusivo del fixture**. Comprobar que el
   panel confirma PIN activo. Nunca introducir un PIN personal.
2. Abrir el documento y mostrarlo con el PIN. Confirmar marcador y fecha;
   abrir la imagen y confirmar el rectángulo coral. Buscar el marcador con
   `Ctrl K` y confirmar el resultado. Actualmente el buscador muestra
   «contenido sensible» en lugar del fragmento incluso estando desbloqueado;
   no confundir ese rótulo con una comprobación del fragmento recibido por IPC.
3. Bloquear desde el candado con el contenido visible. El texto, la imagen, la
   fecha y los demás datos protegidos deben retirarse; nombres/rutas siguen públicos. Repetir con el
   buscador abierto y con una vista previa abierta. Cambiar de vista no debe
   recuperar el contenido.
4. Desbloquear de nuevo y dejar contenido visible. Anotar el instante de
   autenticación y esperar **cinco minutos reales**, realizando lecturas durante
   el plazo. Debe retirarse el contenido al caducar sin una nueva interacción.
   Repetir tras dejar la ventana en segundo plano; al volver no debe conservar
   contenido protegido. No alterar el reloj del sistema ni el plazo del producto.
5. Verificar que abrir/revelar y corregir un documento protegido requieren PIN.
   Los tests controlados cubren respuestas en vuelo y fallos de consulta: una
   inspección visual por sí sola no reproduce esas carreras determinísticamente.

Cerrar sólo esta ventana al terminar. Conservar registro de commit/build,
casos observados y hora de autenticación/caducidad, sin rutas absolutas,
credenciales ni capturas de otras ventanas. La limpieza del fixture es una
operación separada: no borrar rutas calculadas sin comprobar su ubicación y
que esta instancia y sus procesos WebView han terminado.

## Resultado de esta entrega

El resultado observado y las limitaciones se registran en
`PROGRESO-CONFIANZA.md`. Este procedimiento no sustituye una verificación de
MSI/NSIS release ni las pruebas con reloj/transporte controlados.
