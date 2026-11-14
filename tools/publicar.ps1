# Publica una version de Orchestrator's Brain desde esta laptop (ADR-0037).
#
# Compila el instalador, lo firma con la llave de publicacion y lo sube a
# GitHub como version publicada. Las copias instaladas la ofrecen al abrirse.
#
# Lo que hace seguro publicar asi, y las cuatro cosas son deliberadas:
#
#   1. SOLO SE PUBLICA LO QUE ESTA EN main Y SUBIDO. Con cambios sin confirmar
#      o commits sin subir, se para: lo publicado tiene que poder leerse en
#      GitHub tal cual.
#   2. LA LLAVE NO SALE DE LA LAPTOP. Se lee de %USERPROFILE%\.obrain-publicar
#      y su contrasena se pide aqui, a mano, en cada publicacion. No se guarda.
#   3. LA FIRMA LLEVA LA VERSION. La app exige que la firma diga para que
#      version se hizo; si la herramienta no la escribe, no se publica.
#   4. EL ORDEN: primero se compila y se firma todo, y solo al final se crea
#      la etiqueta y la version en GitHub. Un fallo a medias no deja publicada
#      una version rota.
#
# No ejecuta msiexec, no inicia obrain-app.exe y no accede a %USERPROFILE%\.obrain.
#
# Uso (desde la raiz del repositorio):
#   ./tools/publicar.ps1 -Version 0.0.4 -Notas "Arregla el aviso de caducidad"
#   ./tools/publicar.ps1                 # publica la version que ya declara main
#   ./tools/publicar.ps1 -SoloMirar      # dice que haria y sale
param(
    # Version nueva. Si se da, sube el numero en todos los manifiestos, lo
    # confirma en main y lo sube antes de compilar.
    [string]$Version,
    [string]$Notas = '',
    [string]$Llave = (Join-Path $env:USERPROFILE '.obrain-publicar\obrain-publicar.key'),
    [switch]$SoloMirar
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Repositorio = 'jpgarcia04/Orchestrator-s-Brain'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$confPath = Join-Path $repoRoot 'ui\src-tauri\tauri.conf.json'
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Parar([string]$mensaje) { throw "NO SE PUBLICA: $mensaje" }

# No puede llamarse `Git`: PowerShell buscaria la funcion antes que el
# ejecutable. Y lo que git escribe por su salida de errores no es un fallo:
# el fallo lo dice su codigo de salida.
function Invoke-Git {
    $anterior = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $salida = & git.exe -C $repoRoot @args 2>&1
        $codigo = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $anterior
    }
    if ($codigo -ne 0) { Parar "git $($args -join ' ') fallo: $(($salida | Out-String).Trim())" }
    return ($salida | Out-String).Trim()
}

function Leer([string]$ruta) { return [System.IO.File]::ReadAllText($ruta, $utf8) }
function Escribir([string]$ruta, [string]$texto) { [System.IO.File]::WriteAllText($ruta, $texto, $utf8) }

# Sustituye una sola aparicion y exige que exista: un manifiesto que cambie de
# forma no debe quedarse en silencio con la version vieja.
function Sustituir([string]$relativa, [string]$patron, [string]$reemplazo, [int]$veces = 1) {
    $ruta = Join-Path $repoRoot $relativa
    $texto = Leer $ruta
    $hallados = [regex]::Matches($texto, $patron).Count
    if ($hallados -ne $veces) { Parar "${relativa}: se esperaban $veces apariciones de la version y hay $hallados." }
    Escribir $ruta ([regex]::Replace($texto, $patron, $reemplazo))
}

# ------------------------------------------------------------ comprobaciones

Write-Host '==> [1/6] Comprobando el repositorio y la llave...'

if ((Invoke-Git 'rev-parse' '--abbrev-ref' 'HEAD') -ne 'main') { Parar 'hay que estar en la rama main.' }
if (Invoke-Git 'status' '--porcelain') { Parar 'hay cambios sin confirmar.' }
Invoke-Git 'fetch' '--quiet' '--tags' 'origin' | Out-Null
if ((Invoke-Git 'rev-parse' 'HEAD') -ne (Invoke-Git 'rev-parse' 'origin/main')) {
    Parar 'main no coincide con origin/main. Sube o trae los cambios antes.'
}

$conf = Leer $confPath | ConvertFrom-Json
$actual = [string]$conf.version
$pubkey = [string]$conf.plugins.updater.pubkey
if ([string]::IsNullOrWhiteSpace($pubkey)) {
    Parar 'tauri.conf.json no tiene llave publica. Ejecuta antes ./tools/crear-llave-publicacion.ps1'
}
if (-not (Test-Path -LiteralPath $Llave -PathType Leaf)) {
    Parar "no esta la llave de publicacion en $Llave"
}

if (-not $Version) { $Version = $actual }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { Parar "version '$Version' no tiene la forma 1.2.3" }
if ([version]$Version -lt [version]$actual) { Parar "la version $Version es anterior a la actual ($actual)." }
$etiqueta = "v$Version"
if (Invoke-Git 'tag' '--list' $etiqueta) { Parar "la etiqueta $etiqueta ya existe: esa version ya se publico." }

$ErrorActionPreference = 'Continue'
& gh auth status *> $null
$sesion = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($sesion -ne 0) { Parar 'gh no tiene sesion iniciada en GitHub (gh auth login).' }

Write-Host "    Version instalable hoy: $actual   ->   se publica: $Version"
if ($SoloMirar) {
    Write-Host '    (-SoloMirar: no se ha tocado nada)'
    exit 0
}

# ---------------------------------------------------------- subir la version

if ($Version -ne $actual) {
    Write-Host "==> [2/6] Subiendo el numero de version a $Version..."
    $viejo = [regex]::Escape($actual)
    $manifiestos = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'core') -Directory -Filter 'obrain-*' |
        ForEach-Object { "core\$($_.Name)\Cargo.toml" }) + @('ui\src-tauri\Cargo.toml')
    foreach ($m in $manifiestos) {
        Sustituir $m ('(?m)^version = "' + $viejo + '"') ('version = "' + $Version + '"')
    }
    $propios = '(name = "obrain-[a-z]+"\r?\nversion = )"' + $viejo + '"'
    Sustituir 'core\Cargo.lock' $propios ('${1}"' + $Version + '"') ($manifiestos.Count - 1)
    Sustituir 'ui\src-tauri\Cargo.lock' $propios ('${1}"' + $Version + '"') $manifiestos.Count
    Sustituir 'ui\src-tauri\tauri.conf.json' ('"version": "' + $viejo + '"') ('"version": "' + $Version + '"')
    Sustituir 'ui\package.json' ('"version": "' + $viejo + '"') ('"version": "' + $Version + '"')
    Sustituir 'ui\package-lock.json' ('("name": "obrain-ui",\s*"version": )"' + $viejo + '"') ('${1}"' + $Version + '"') 2
    Sustituir 'core\obrain-api\tests\version_coherente.rs' ('let version_esperada = "' + $viejo + '";') ('let version_esperada = "' + $Version + '";')

    Invoke-Git 'add' '--all' | Out-Null
    Invoke-Git 'commit' '--quiet' '-m' "chore(release): version $Version" | Out-Null
    Invoke-Git 'push' '--quiet' 'origin' 'main' | Out-Null
} else {
    Write-Host '==> [2/6] La version ya esta declarada en main: no se toca.'
}
$commit = Invoke-Git 'rev-parse' 'HEAD'

# ------------------------------------------------------- compilar y firmar

Write-Host '==> [3/6] Contrasena de la llave de publicacion.'
$segura = Read-Host -AsSecureString '    Contrasena'
$puntero = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($segura)

$destino = Join-Path $env:USERPROFILE 'obrain-target'
$entrega = Join-Path $env:TEMP "obrain-publicar-$Version"
if (Test-Path -LiteralPath $entrega) { Remove-Item -LiteralPath $entrega -Recurse -Force }
New-Item -ItemType Directory -Force -Path $entrega | Out-Null
# Que se generen los archivos de actualizacion firmados se pide solo aqui: asi
# una compilacion corriente no necesita la llave.
$ajuste = Join-Path $entrega 'publicar.conf.json'
Escribir $ajuste '{ "bundle": { "createUpdaterArtifacts": true } }'

Write-Host '==> [4/6] Compilando y firmando el instalador (varios minutos)...'
$msiDir = Join-Path $destino 'release\bundle\msi'
if (Test-Path -LiteralPath $msiDir) { Remove-Item -LiteralPath $msiDir -Recurse -Force }
Push-Location (Join-Path $repoRoot 'ui')
try {
    # OpenSSL se compila dentro del programa y pide el Perl de Strawberry, y el
    # apostrofo de la carpeta del repositorio rompe su compilacion: se compila fuera.
    $env:PATH = 'C:\Strawberry\perl\bin;C:\Strawberry\c\bin;' + $env:PATH
    $env:CARGO_TARGET_DIR = $destino
    $env:TAURI_SIGNING_PRIVATE_KEY = (Leer $Llave).Trim()
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($puntero)
    & npm ci --silent
    if ($LASTEXITCODE -ne 0) { Parar 'npm ci fallo.' }
    & npx tauri build --bundles msi --config $ajuste
    if ($LASTEXITCODE -ne 0) { Parar 'la compilacion o la firma fallaron (contrasena equivocada?).' }
} finally {
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($puntero)
    Pop-Location
}

$msi = @(Get-ChildItem -LiteralPath $msiDir -Filter "*_${Version}_*.msi" -File)
if ($msi.Count -ne 1) { Parar "se esperaba un instalador $Version en $msiDir y hay $($msi.Count)." }
$firmaRuta = "$($msi[0].FullName).sig"
if (-not (Test-Path -LiteralPath $firmaRuta)) { Parar 'el instalador no salio firmado.' }
$firma = (Leer $firmaRuta).Trim()

Write-Host '==> [5/6] Comprobando que la firma lleva la version...'
$firmaTexto = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($firma))
if ($firmaTexto -notmatch ("(?m)^trusted comment:.*\tversion:" + [regex]::Escape($Version) + '(\t|\s*$)')) {
    Parar "la firma no declara la version $Version. Actualiza @tauri-apps/cli: la app rechazaria esta version."
}

# Nombres sin espacios ni apostrofo: GitHub los cambia al subirlos y la
# direccion de descarga tiene que coincidir con la de latest.json.
$nombre = "obrain_${Version}_x64.msi"
Copy-Item -LiteralPath $msi[0].FullName -Destination (Join-Path $entrega $nombre)
$hash = (Get-FileHash -LiteralPath (Join-Path $entrega $nombre) -Algorithm SHA256).Hash
$manifiesto = [ordered]@{
    version   = $Version
    notes     = $Notas
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $firma
            url       = "https://github.com/$Repositorio/releases/download/$etiqueta/$nombre"
        }
    }
}
Escribir (Join-Path $entrega 'latest.json') ($manifiesto | ConvertTo-Json -Depth 5)

# --------------------------------------------------------------- publicar

Write-Host "==> [6/6] Publicando $etiqueta en GitHub..."
Invoke-Git 'tag' $etiqueta $commit | Out-Null
Invoke-Git 'push' '--quiet' 'origin' $etiqueta | Out-Null
$cuerpo = if ($Notas) { $Notas } else { "Version $Version." }
$cuerpo = "$cuerpo`n`nSHA-256 de ${nombre}: $hash"
& gh release create $etiqueta (Join-Path $entrega $nombre) (Join-Path $entrega 'latest.json') `
    --repo $Repositorio --title "Orchestrator's Brain $Version" --notes $cuerpo --verify-tag --latest
if ($LASTEXITCODE -ne 0) {
    Parar "la etiqueta $etiqueta se subio pero la version no se pudo crear en GitHub. Los archivos estan en $entrega"
}

Write-Host ''
Write-Host "LISTO. $etiqueta publicada desde el commit $($commit.Substring(0, 7))."
Write-Host '       Las copias instaladas la ofreceran la proxima vez que se abran.'
Write-Host "       Instalador (para instalar a mano): $(Join-Path $entrega $nombre)"
