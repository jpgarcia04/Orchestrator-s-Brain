# Verificacion local y de CI ligera (Niveles A y B) - Orchestrator's Brain
# No ejecuta msiexec, no inicia obrain-app.exe y no accede a %USERPROFILE%\.obrain.
param(
    [switch]$SoloPoliticaCI,
    [switch]$ModoCI,
    [switch]$Completo
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Assert-Check([bool]$condition, [string]$message) {
    if (-not $condition) {
        throw "FALLO DE VERIFICACION: $message"
    }
}

Write-Host "==> [1/5] Verificando coherencia de versiones entre core/, ui/ y tauri.conf.json..."
$tauriConf = Get-Content -LiteralPath (Join-Path $repoRoot 'ui/src-tauri/tauri.conf.json') -Encoding UTF8 -Raw | ConvertFrom-Json
$expectedVersion = [string]$tauriConf.version
Assert-Check (-not [string]::IsNullOrWhiteSpace($expectedVersion)) 'ui/src-tauri/tauri.conf.json no define version.'

$uiPkg = Get-Content -LiteralPath (Join-Path $repoRoot 'ui/package.json') -Encoding UTF8 -Raw | ConvertFrom-Json
Assert-Check ([string]$uiPkg.version -eq $expectedVersion) "ui/package.json ($($uiPkg.version)) != tauri.conf.json ($expectedVersion)"

$uiPkgLockText = Get-Content -LiteralPath (Join-Path $repoRoot 'ui/package-lock.json') -Encoding UTF8 -Raw
Assert-Check ($uiPkgLockText -match ('"name"\s*:\s*"obrain-ui"\s*,\s*"version"\s*:\s*"' + [regex]::Escape($expectedVersion) + '"')) "ui/package-lock.json no declara version $expectedVersion"

$cargoTomls = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'core') -Recurse -Filter 'Cargo.toml' -File | Where-Object { $_.FullName -notmatch '[\\/]target[\\/]' })
$cargoTomls += Get-Item -LiteralPath (Join-Path $repoRoot 'ui/src-tauri/Cargo.toml')
foreach ($toml in $cargoTomls) {
    $text = Get-Content -LiteralPath $toml.FullName -Encoding UTF8 -Raw
    if ($text -match '(?m)^\[package\]') {
        Assert-Check ($text -match ('(?m)^version\s*=\s*"' + [regex]::Escape($expectedVersion) + '"')) "Version incoherente en $($toml.FullName) (esperada: $expectedVersion)"
    }
}
Write-Host "    OK: Version coherente ($expectedVersion) en todos los manifiestos."

Write-Host "==> [2/5] Verificando sintaxis AST de scripts PowerShell en tools/..."
$psScripts = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'tools') -Filter '*.ps1' -File)
Assert-Check ($psScripts.Count -gt 0) 'No se encontraron scripts .ps1 en tools/.'
foreach ($script in $psScripts) {
    $parseErrors = $null
    $scriptContent = [System.IO.File]::ReadAllText($script.FullName, [System.Text.Encoding]::UTF8)
    [System.Management.Automation.Language.Parser]::ParseInput($scriptContent, $script.FullName, [ref]$null, [ref]$parseErrors) | Out-Null
    if ($parseErrors -and $parseErrors.Count -gt 0) {
        $detail = ($parseErrors | ForEach-Object { $_.ToString() }) -join '; '
        throw "Error de sintaxis PowerShell en $($script.Name): $detail"
    }
    if ($script.Name -ne 'verificar-local.ps1' -and $scriptContent -match '(?<!@)\(\$[A-Za-z0-9_]+\s+-match\s+[^)]+\)\.Count') {
        throw "Patron inseguro bajo StrictMode en $($script.Name): usa @(`$var -match ...).Count en lugar de (`$var -match ...).Count"
    }
}
Write-Host "    OK: $($psScripts.Count) scripts PowerShell analizados sin errores de sintaxis."

Write-Host "==> [3/5] Verificando politica de eficiencia de GitHub Actions (.github/workflows/)..."
$workflowsDir = Join-Path $repoRoot '.github/workflows'
$msiWorkflowPath = Join-Path $workflowsDir 'validar-msi-windows.yml'
$prWorkflowPath = Join-Path $workflowsDir 'verificar-pr.yml'
Assert-Check (Test-Path -LiteralPath $msiWorkflowPath -PathType Leaf) 'Falta .github/workflows/validar-msi-windows.yml'
Assert-Check (Test-Path -LiteralPath $prWorkflowPath -PathType Leaf) 'Falta .github/workflows/verificar-pr.yml'

$msiWf = Get-Content -LiteralPath $msiWorkflowPath -Encoding UTF8 -Raw
# Regla 1: El workflow pesado de Windows MSI nunca debe dispararse automaticamente en pull_request ni push.
Assert-Check ($msiWf -match '(?m)^on:\s*\r?\n\s+workflow_dispatch:') 'validar-msi-windows.yml debe activarse exclusivamente mediante workflow_dispatch.'
Assert-Check ($msiWf -notmatch '(?m)^\s*pull_request\s*:') 'PROHIBIDO: validar-msi-windows.yml contiene un disparador pull_request automatico.'
Assert-Check ($msiWf -notmatch '(?m)^\s*push\s*:') 'PROHIBIDO: validar-msi-windows.yml contiene un disparador push automatico.'
# Regla 2: Control de concurrencia, permisos minimos y timeout acotado.
Assert-Check ($msiWf -match '(?m)^concurrency:' -and $msiWf -match 'cancel-in-progress:\s*true') 'validar-msi-windows.yml debe declarar concurrency con cancel-in-progress: true.'
Assert-Check ($msiWf -match 'permissions:\s*\r?\n\s+contents:\s*read') 'validar-msi-windows.yml debe mantener permissions: contents: read.'
Assert-Check ($msiWf -match 'timeout-minutes:\s*(70|65|60|[1-5][0-9])') 'validar-msi-windows.yml debe tener un timeout-minutes acotado (<= 70).'
# Regla 3: Preflight del MSI 0.0.1 antes de compilar Tauri/Rust para no desperdiciar minutos si falta el insumo.
$idxPreflight001 = $msiWf.IndexOf('Preflight y descarga de MSI 0.0.1 antes de compilar')
$idxTauriBuild = $msiWf.IndexOf('npm run tauri -- build')
Assert-Check ($idxPreflight001 -gt 0 -and $idxTauriBuild -gt $idxPreflight001) 'validar-msi-windows.yml debe ejecutar el preflight/descarga de MSI 0.0.1 antes de compilar Tauri.'

$prWf = Get-Content -LiteralPath $prWorkflowPath -Encoding UTF8 -Raw
Assert-Check ($prWf -match 'runs-on:\s*ubuntu-latest') 'verificar-pr.yml debe ejecutarse unicamente en ubuntu-latest (multiplicador 1x).'
Assert-Check ($prWf -notmatch 'runs-on:\s*windows') 'PROHIBIDO: verificar-pr.yml no puede utilizar runners Windows.'
Assert-Check ($prWf -match '(?m)^concurrency:' -and $prWf -match 'cancel-in-progress:\s*true') 'verificar-pr.yml debe declarar concurrency con cancel-in-progress: true.'
Assert-Check ($prWf -match 'timeout-minutes:\s*(10|[1-9])\b') 'verificar-pr.yml debe tener un timeout-minutes <= 10.'
Write-Host "    OK: Politica de eficiencia de CI verificada en los workflows."

Write-Host "==> [4/5] Verificando salvaguarda de datos personales y archivos privados en git..."
$stagedFiles = @()
$unstagedFiles = @()
try {
    $stagedFiles += @(git -C $repoRoot diff --cached --name-only 2>$null)
    $unstagedFiles += @(git -C $repoRoot diff --name-only 2>$null)
    $unstagedFiles += @(git -C $repoRoot diff --name-only origin/main...HEAD 2>$null)
} catch {}
$filesToCheck = @($stagedFiles + $unstagedFiles | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)

$forbiddenExtRegex = '(?i)\.(llave|obrain|sqlite|db|onnx|log)$'
$forbiddenPathRegex = '(?i)(\.obrain([\\/]|$)|^obrain-respaldo|^obrain-ensayo|^obrain-salvaguarda|\.transicion\.json$)'

foreach ($f in $filesToCheck) {
    if ($f -match $forbiddenExtRegex -or $f -match $forbiddenPathRegex) {
        throw "SALVAGUARDA DE PRIVACIDAD: El archivo '$f' corresponde a datos privados, llaves o artefactos de ejecucion y no debe ser incorporado a Git."
    }
}

$textFilesToCheck = @($filesToCheck | Where-Object { $_ -notmatch '(?i)\.(png|ico|jpg|jpeg|gif|webp|woff2?|exe|dll|msi|cab|zip|pdf)$' -and $_ -notmatch '(?i)^docs/archivo/' })
$personalPathPattern = '(?i)[a-z]:\\users\\pacox'

foreach ($tf in $textFilesToCheck) {
    $fullPath = Join-Path $repoRoot $tf
    if (Test-Path -LiteralPath $fullPath -PathType Leaf) {
        $content = [System.IO.File]::ReadAllText($fullPath, [System.Text.Encoding]::UTF8)
        if ($content -match $personalPathPattern) {
            if ($tf -ne 'core/obrain-scan/src/exclude.rs') {
                throw "SALVAGUARDA DE PRIVACIDAD: El archivo '$tf' contiene una ruta personal anfitriona no permitida. Usa resolucion dinamica con variables de entorno."
            }
        }
    }
}
Write-Host "    OK: Salvaguarda de datos personales verificada ($($filesToCheck.Count) archivos analizados, 0 datos privados detectados)."

if ($SoloPoliticaCI) {
    Write-Host "Verificacion rapida de politica CI, salvaguarda de datos y coherencia completada."
    exit 0
}

Write-Host "==> [5/5] Verificando formato de archivos Rust modificados y tipado/pruebas de interfaz..."
$changedRs = @()
try {
    $changedRs += @(git -C $repoRoot diff --name-only origin/main...HEAD -- "*.rs" 2>$null)
    $changedRs += @(git -C $repoRoot diff --name-only -- "*.rs" 2>$null)
} catch {}
$changedRs = @($changedRs | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
foreach ($relRs in $changedRs) {
    $fullRs = Join-Path $repoRoot $relRs
    if (Test-Path -LiteralPath $fullRs -PathType Leaf) {
        rustfmt --edition 2024 --check $fullRs
        Assert-Check ($LASTEXITCODE -eq 0) "rustfmt --check fallo en $relRs"
    }
}
Write-Host "    OK: Formato Rust verificado ($($changedRs.Count) archivos .rs modificados en la rama)."

Push-Location (Join-Path $repoRoot 'ui')
try {
    npm run check
    Assert-Check ($LASTEXITCODE -eq 0) 'npm run check (svelte-check) fallo en ui/.'
    npm test
    Assert-Check ($LASTEXITCODE -eq 0) 'npm test fallo en ui/.'
    npm run build
    Assert-Check ($LASTEXITCODE -eq 0) 'npm run build fallo en ui/.'
} finally {
    Pop-Location
}

if ($Completo) {
    Write-Host "==> [Nivel A Completo] Ejecutando pruebas de coherencia Rust..."
    if (-not $env:CARGO_TARGET_DIR) {
        if ($env:USERPROFILE) {
            $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE 'obrain-target'
        } else {
            $env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'obrain-target'
        }
    }
    cargo test --manifest-path (Join-Path $repoRoot 'core/Cargo.toml') --locked -p obrain-api --test version_coherente
    Assert-Check ($LASTEXITCODE -eq 0) 'Prueba version_coherente fallo.'
}

Write-Host "Verificacion completada satisfactoriamente."
