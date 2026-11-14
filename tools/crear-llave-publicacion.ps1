# Crea la llave con la que se firman las versiones publicadas (ADR-0037).
#
# Se ejecuta UNA vez, en la laptop de quien publica. Pide una contrasena dos
# veces (la pide la herramienta de Tauri, no este script, que nunca la ve).
#
#   - La llave PRIVADA queda fuera del repositorio y fuera de OneDrive:
#       %USERPROFILE%\.obrain-publicar\obrain-publicar.key
#     Quien la tenga, con su contrasena, puede publicar versiones que la app
#     instalara. No se copia a GitHub ni a ningun servicio.
#   - La llave PUBLICA se escribe en ui/src-tauri/tauri.conf.json. Esa si va
#     al repositorio: solo sirve para comprobar firmas.
#
# Si la llave privada se pierde, las copias ya instaladas no pueden volver a
# actualizarse solas: hay que crear otra e instalar a mano una vez mas.
#
# Uso (desde la raiz del repositorio):
#   ./tools/crear-llave-publicacion.ps1
param(
    [string]$Llave = (Join-Path $env:USERPROFILE '.obrain-publicar\obrain-publicar.key')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$confPath = Join-Path $repoRoot 'ui\src-tauri\tauri.conf.json'
$utf8 = New-Object System.Text.UTF8Encoding($false)

if (Test-Path -LiteralPath $Llave) {
    throw "Ya existe una llave en $Llave. No se sobrescribe: las copias instaladas dejarian de actualizarse. Si de verdad quieres otra, mueve antes la actual."
}

$carpeta = Split-Path -Parent $Llave
if (-not (Test-Path -LiteralPath $carpeta)) {
    New-Item -ItemType Directory -Force -Path $carpeta | Out-Null
}

Write-Host '==> Creando la llave de publicacion.'
Write-Host '    Elige una contrasena y guardala en tu gestor de contrasenas: se pide en cada publicacion.'
Push-Location (Join-Path $repoRoot 'ui')
try {
    & npx tauri signer generate -w $Llave
    if ($LASTEXITCODE -ne 0) { throw 'La herramienta de Tauri no pudo crear la llave.' }
} finally {
    Pop-Location
}

$publica = "$Llave.pub"
if (-not (Test-Path -LiteralPath $publica)) {
    throw "No aparecio la llave publica en $publica."
}
$pubkey = ([System.IO.File]::ReadAllText($publica, $utf8)).Trim()
if ([string]::IsNullOrWhiteSpace($pubkey)) { throw 'La llave publica esta vacia.' }

# Se sustituye solo la linea de la llave: el resto del archivo no se reescribe.
$texto = [System.IO.File]::ReadAllText($confPath, $utf8)
$patron = '"pubkey"\s*:\s*"[^"]*"'
if ($texto -notmatch $patron) {
    throw 'ui/src-tauri/tauri.conf.json no tiene plugins.updater.pubkey.'
}
$texto = [regex]::Replace($texto, $patron, ('"pubkey": "' + $pubkey + '"'), 1)
[System.IO.File]::WriteAllText($confPath, $texto, $utf8)

Write-Host ''
Write-Host "LISTO. Llave privada: $Llave"
Write-Host '       Llave publica escrita en ui/src-tauri/tauri.conf.json (hay que confirmarla en git).'
Write-Host '       Guarda una copia de la llave privada en un sitio que no sea esta laptop.'
