# Inspección estática y segura del candidato MSI sin instalarlo en el equipo anfitrión.
param(
    [string]$CandidateMsiPath = $null,
    [string]$Msi001Path = $null,
    [string]$ExpectedCommit = $null
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-True([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

function Read-MsiRows($database, [string]$sql, [int]$columns) {
    $view = $database.OpenView($sql)
    try {
        $view.Execute() | Out-Null
        $rows = @()
        while ($record = $view.Fetch()) {
            $row = @()
            for ($i = 1; $i -le $columns; $i++) { $row += [string]$record.StringData($i) }
            $rows += ,$row
        }
        return $rows
    } finally {
        $view.Close() | Out-Null
    }
}

# 1. Localizar candidato MSI 0.0.2
if (-not $CandidateMsiPath -and $env:OBRAIN_MSI_PATH) {
    $CandidateMsiPath = $env:OBRAIN_MSI_PATH
}
if (-not $CandidateMsiPath) {
    $searchPaths = @()
    if ($env:CARGO_TARGET_DIR) {
        $searchPaths += (Join-Path $env:CARGO_TARGET_DIR 'release\bundle\msi')
    }
    if ($env:RUNNER_TEMP) {
        $searchPaths += (Join-Path $env:RUNNER_TEMP 'obrain-cargo-target\release\bundle\msi')
    }
    if ($env:TEMP) {
        $searchPaths += (Join-Path $env:TEMP 'obrain-cargo-target\release\bundle\msi')
    }
    $searchPaths += @(
        'C:\Users\pacox\obrain-target\release\bundle\msi',
        (Join-Path $PSScriptRoot '..\ui\src-tauri\target\release\bundle\msi')
    )
    foreach ($dir in $searchPaths) {
        if ($dir -and (Test-Path -LiteralPath $dir)) {
            $found = @(Get-ChildItem -LiteralPath $dir -Filter '*0.0.2*.msi' -File)
            if ($found.Count -eq 0) {
                $found = @(Get-ChildItem -LiteralPath $dir -Filter '*.msi' -File)
            }
            if ($found.Count -gt 0) {
                $CandidateMsiPath = $found[0].FullName
                break
            }
        }
    }
}

Assert-True ([bool]$CandidateMsiPath -and (Test-Path -LiteralPath $CandidateMsiPath -PathType Leaf)) "No se localizó el paquete MSI 0.0.2. Ruta evaluada: $CandidateMsiPath"

$candidateItem = Get-Item -LiteralPath $CandidateMsiPath
$candidateHash = (Get-FileHash -LiteralPath $CandidateMsiPath -Algorithm SHA256).Hash
$candidateSig = (Get-AuthenticodeSignature -LiteralPath $CandidateMsiPath).Status.ToString()

# 2. Inspección COM en modo sólo lectura (modo 0)
$installer = New-Object -ComObject WindowsInstaller.Installer
$candidateDb = $installer.OpenDatabase($CandidateMsiPath, 0)

$candidateProps = @{}
foreach ($row in (Read-MsiRows $candidateDb 'SELECT `Property`, `Value` FROM `Property`' 2)) {
    $candidateProps[$row[0]] = $row[1]
}

$tables = @((Read-MsiRows $candidateDb 'SELECT `Name` FROM `_Tables`' 1) | ForEach-Object { $_[0] })
$files = @((Read-MsiRows $candidateDb 'SELECT `FileName`, `FileSize` FROM `File`' 2) | ForEach-Object { "$($_[0]) ($($_[1]) bytes)" })
$upgradeRows = @(Read-MsiRows $candidateDb 'SELECT `UpgradeCode`, `VersionMin`, `VersionMax`, `Attributes`, `ActionProperty` FROM `Upgrade`' 5)

Assert-True ($candidateProps['ProductName'] -eq "Orchestrator's Brain") "ProductName inesperado: $($candidateProps['ProductName'])"
Assert-True ($candidateProps['ProductVersion'] -eq '0.0.2') "ProductVersion inesperado: $($candidateProps['ProductVersion'])"
Assert-True ($candidateProps['ProductCode'] -match '^\{[0-9A-Fa-f-]{36}\}$') "ProductCode ausente o inválido: $($candidateProps['ProductCode'])"
Assert-True ($candidateProps['UpgradeCode'] -eq '{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}') "UpgradeCode no coincide con el identificador del producto Brain"
Assert-True ($candidateProps['ALLUSERS'] -eq '1') 'ALLUSERS debe ser 1 (instalación por máquina)'
Assert-True ($tables -notcontains 'ServiceInstall') 'El paquete no debe declarar ServiceInstall'
Assert-True (($files -match 'obrain-app\.exe').Count -eq 1) 'obrain-app.exe debe figurar exactamente una vez'
Assert-True (($files -match 'DirectML\.dll').Count -eq 1) 'DirectML.dll debe figurar exactamente una vez'
Assert-True ($upgradeRows.Count -gt 0) 'La tabla Upgrade debe declarar al menos una regla de actualización mayor'

# 3. Comprobar compatibilidad con MSI 0.0.1 auténtico
$msi001Report = [ordered]@{
    disponible = $false
    ruta = $null
    sha256 = $null
    sha256Autentico = '9AEE1B8125CC19DDBBF124350CD86EB510E77A9313C89094E0797A6AF7E35591'
    sha256Coincide = $false
    productCode = $null
    upgradeCode = $null
    version = $null
    actualizacionMayorValida = $false
}

if (-not $Msi001Path) {
    $posibles001 = @(
        'C:\Users\pacox\obrain-msi-0.0.1\Orchestrator''s Brain_0.0.1_x64_en-US.msi',
        'C:\Users\pacox\obrain-target\release\bundle\msi\Orchestrator''s Brain_0.0.1_x64_en-US.msi',
        (Join-Path $env:TEMP 'Orchestrator''s Brain_0.0.1_x64_en-US.msi'),
        (Join-Path $PSScriptRoot '..\tests\fixtures\msi-0.0.1\Orchestrator''s Brain_0.0.1_x64_en-US.msi')
    )
    foreach ($p in $posibles001) {
        if (Test-Path -LiteralPath $p -PathType Leaf) {
            $Msi001Path = $p
            break
        }
    }
}

if ($Msi001Path -and (Test-Path -LiteralPath $Msi001Path -PathType Leaf)) {
    $msi001Report.disponible = $true
    $msi001Report.ruta = $Msi001Path
    $hash001 = (Get-FileHash -LiteralPath $Msi001Path -Algorithm SHA256).Hash
    $msi001Report.sha256 = $hash001
    $msi001Report.sha256Coincide = ($hash001 -eq $msi001Report.sha256Autentico)

    if ($msi001Report.sha256Coincide) {
        $db001 = $installer.OpenDatabase($Msi001Path, 0)
        $props001 = @{}
        foreach ($row in (Read-MsiRows $db001 'SELECT `Property`, `Value` FROM `Property`' 2)) {
            $props001[$row[0]] = $row[1]
        }
        $msi001Report.productCode = $props001['ProductCode']
        $msi001Report.upgradeCode = $props001['UpgradeCode']
        $msi001Report.version = $props001['ProductVersion']

        # Validación estructural de Major Upgrade:
        # 1. Mismo UpgradeCode
        # 2. Distinto ProductCode
        # 3. ProductVersion 0.0.2 > 0.0.1
        $mismoUpgradeCode = ($candidateProps['UpgradeCode'] -eq $props001['UpgradeCode'])
        $distintoProductCode = ($candidateProps['ProductCode'] -ne $props001['ProductCode'])
        $versionMayor = ([version]$candidateProps['ProductVersion'] -gt [version]$props001['ProductVersion'])

        $msi001Report.actualizacionMayorValida = ($mismoUpgradeCode -and $distintoProductCode -and $versionMayor)
    }
}

$resultado = [ordered]@{
    candidato = [ordered]@{
        archivo = $candidateItem.Name
        ruta = $candidateItem.FullName
        bytes = $candidateItem.Length
        sha256 = $candidateHash
        firma = $candidateSig
        productName = $candidateProps['ProductName']
        productVersion = $candidateProps['ProductVersion']
        productCode = $candidateProps['ProductCode']
        upgradeCode = $candidateProps['UpgradeCode']
        allUsers = $candidateProps['ALLUSERS']
        archivos = $files
        sinServicio = ($tables -notcontains 'ServiceInstall')
    }
    referencia001 = $msi001Report
    commitEvaluado = $ExpectedCommit
    verificacionEstatica = 'PASO'
}

$resultado | ConvertTo-Json -Depth 6
