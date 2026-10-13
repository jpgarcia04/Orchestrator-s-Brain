param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$Database,
    [Parameter(Mandatory = $true)][string]$Executable,
    [switch]$AlreadyMigrated,
    [switch]$PreflightOnly
)

$ErrorActionPreference = 'Stop'
$rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\')
$dbPath = [IO.Path]::GetFullPath($Database)
$exePath = [IO.Path]::GetFullPath($Executable)
$envKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
if (-not $envKey) { throw 'No se pudo leer el entorno del usuario de Windows' }
try {
    $userTemp = $envKey.GetValue('TEMP', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
} finally {
    $envKey.Close()
}
if (-not $userTemp) { throw 'No se pudo determinar TEMP del usuario de Windows' }
$userTemp = $userTemp.Replace('%USERPROFILE%', [Environment]::GetFolderPath('UserProfile'), [StringComparison]::OrdinalIgnoreCase)
$userTemp = $userTemp.Replace('%LOCALAPPDATA%', [Environment]::GetFolderPath('LocalApplicationData'), [StringComparison]::OrdinalIgnoreCase)
if ($userTemp.Contains('%')) { throw 'TEMP del usuario contiene variables no resueltas' }
$realTemp = [IO.Path]::GetFullPath($userTemp).TrimEnd('\')

function Assert-Child([string]$Path, [string]$Parent) {
    if (-not $Path.StartsWith($Parent + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Ruta fuera del aislamiento: $Path"
    }
}

Assert-Child $rootPath $realTemp
Assert-Child $dbPath $rootPath
if (-not (Test-Path -LiteralPath $dbPath -PathType Leaf)) { throw "Falta base sintética: $dbPath" }
if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) { throw "Falta ejecutable: $exePath" }
if ((Get-Item -LiteralPath $rootPath).Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) {
    throw 'La raíz aislada no puede ser un enlace o punto de reanálisis'
}
$pathProbe = @'
import os, sys
root, db = map(os.path.realpath, sys.argv[1:])
if os.path.commonpath((db, root)) != root:
    raise SystemExit('La base resuelve fuera del aislamiento')
'@
& python -c $pathProbe $rootPath $dbPath
if ($LASTEXITCODE -ne 0) { throw 'La ruta real de la base no es segura' }

if (-not $AlreadyMigrated) {
    $probe = @'
import os, pathlib, sqlite3, sys
db, root = sys.argv[1:]
with sqlite3.connect(pathlib.Path(db).as_uri() + '?mode=ro', uri=True) as conn:
    version = conn.execute('PRAGMA user_version').fetchone()[0]
    paths = [row[0] for row in conn.execute("SELECT object_text FROM assertions WHERE predicate IN ('folder_path','path_observed_at') AND retracted_at IS NULL")]
    tables = {row[0] for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
if version != 1 or len(paths) != 3 or 'content_publication' in tables:
    raise SystemExit('La base no es la fixture v1 esperada')
real_root = os.path.realpath(root)
if os.path.commonpath((os.path.realpath(db), real_root)) != real_root:
    raise SystemExit('La base resuelve fuera del aislamiento')
for path in paths:
    if os.path.commonpath((os.path.realpath(path), real_root)) != real_root:
        raise SystemExit('La fixture contiene una ruta ajena al aislamiento: ' + path)
print('DDL v1 y rutas de biblioteca sintéticas comprobados')
'@
    & python -c $probe $dbPath $rootPath
    if ($LASTEXITCODE -ne 0) { throw 'Falló la inspección de la fixture v1' }
} elseif (-not (Test-Path -LiteralPath ($dbPath + '.llave') -PathType Leaf)) {
    throw 'Falta la llave sintética de la base migrada'
}

$profile = Join-Path $rootPath 'profile'
$local = Join-Path $rootPath 'localappdata'
$roaming = Join-Path $rootPath 'appdata'
$temp = Join-Path $rootPath 'temp'
$webview = Join-Path $rootPath 'webview2'
foreach ($dir in @($profile, $local, $roaming, $temp, $webview)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Assert-Child ([IO.Path]::GetFullPath($dir)) $rootPath
}

$env:OBRAIN_DB = $dbPath
$env:USERPROFILE = $profile
$env:HOME = $profile
$env:LOCALAPPDATA = $local
$env:APPDATA = $roaming
$env:TEMP = $temp
$env:TMP = $temp
$env:XDG_DATA_HOME = Join-Path $rootPath 'xdg-data'
$env:XDG_CACHE_HOME = Join-Path $rootPath 'xdg-cache'
$env:WEBVIEW2_USER_DATA_FOLDER = $webview
$env:RUST_BACKTRACE = 'full'

Write-Output "Aislamiento verificado: DB=$env:OBRAIN_DB; USERPROFILE=$env:USERPROFILE; LOCALAPPDATA=$env:LOCALAPPDATA; WEBVIEW2_USER_DATA_FOLDER=$env:WEBVIEW2_USER_DATA_FOLDER"
if (-not $PreflightOnly) { & $exePath }
