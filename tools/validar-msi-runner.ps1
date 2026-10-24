# Ejecutar sólo en el runner Windows temporal del workflow.
param(
    [string]$MsiPath = $null,
    [string]$Msi001Path = $null
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$evidence = Join-Path $env:RUNNER_TEMP 'obrain-msi-evidence'
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$report = [ordered]@{
    commit = $env:GITHUB_SHA
    runnerImage = $env:ImageOS
    runnerImageVersion = $env:ImageVersion
    os = [Environment]::OSVersion.VersionString
    windowsInstaller = $null
    webView2 = $null
    msiCandidate = $null
    msi001 = $null
    preflight = @()
    cleanInstall = [ordered]@{
        status = 'no_iniciado'
        install = $null
        installed = $null
        launch = $null
        uninstall = $null
    }
    upgradeFrom001 = [ordered]@{
        status = 'pendiente'
        reason = $null
        install001 = $null
        installed001 = $null
        launch001 = $null
        upgrade = $null
        installed002 = $null
        launch002 = $null
        knowledgeRecovery = $null
        uninstall002 = $null
    }
    limitations = @(
        'No se automatizan clics, capturas ni inspeccion visual de la interfaz.',
        'La ausencia de escrituras en rutas ajenas no demuestra ausencia de lecturas; no se traza acceso a archivos del proceso.'
    )
    error = $null
}

function Assert-True([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

function Invoke-Msiexec([string]$operation, [string]$target, [string]$log) {
    $arguments = '{0} "{1}" /qn /norestart /L*V "{2}"' -f $operation, $target, $log
    $process = Start-Process -FilePath (Get-Command msiexec.exe).Source -ArgumentList $arguments -Wait -PassThru
    return $process.ExitCode
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

function Get-WebView2Version {
    $id = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($root in @('HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients',
                         'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients',
                         'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients')) {
        $key = Join-Path $root $id
        $version = (Get-ItemProperty -LiteralPath $key -Name pv -ErrorAction SilentlyContinue).pv
        if ($version -and $version -ne '0.0.0.0') { return [string]$version }
    }
    return $null
}

function Get-UninstallEntry([string]$code) {
    foreach ($root in @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
                         'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
                         'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
                         'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall')) {
        $key = Join-Path $root $code
        if (Test-Path -LiteralPath $key) { return Get-ItemProperty -LiteralPath $key }
    }
    return $null
}

$msi = $null
$productCode = $null
$installAttempted = $false
$installedProductCodesToCleanup = @()
$fixtureRoot = $null
$originalProfile = $env:USERPROFILE
$originalLocalAppData = $env:LOCALAPPDATA
$failure = $null

try {
    Assert-True ($env:GITHUB_ACTIONS -eq 'true' -and $env:RUNNER_ENVIRONMENT -eq 'github-hosted') 'Sólo se permite un runner hospedado por GitHub Actions.'
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    Assert-True ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'El token del runner no es administrador; no se instala el MSI.'
    $service = Get-Service -Name msiserver -ErrorAction Stop
    $report.windowsInstaller = [ordered]@{ service = $service.Status.ToString(); msiexec = (Get-Command msiexec.exe).Source }
    Assert-True ($service.StartType -ne 'Disabled') 'Windows Installer está deshabilitado.'
    $report.preflight += 'Runner hospedado, token de administrador y msiexec presentes.'

    $defaultDbDir = Join-Path $originalProfile '.obrain'
    $oldDbDir = Join-Path $originalLocalAppData 'obrain'
    Assert-True (-not (Test-Path -LiteralPath $defaultDbDir)) 'El perfil original del runner ya contiene .obrain.'
    Assert-True (-not (Test-Path -LiteralPath $oldDbDir)) 'AppData original del runner ya contiene obrain.'
    $report.preflight += 'Las ubicaciones Brain originales del runner no existen.'

    # Identificar MSI candidato 0.0.2
    if (-not $MsiPath) {
        $bundleDir = Join-Path $env:CARGO_TARGET_DIR 'release\bundle\msi'
        if (Test-Path -LiteralPath $bundleDir) {
            $candidatos = @(Get-ChildItem -LiteralPath $bundleDir -Filter '*0.0.2*.msi' -File)
            if ($candidatos.Count -eq 0) {
                $candidatos = @(Get-ChildItem -LiteralPath $bundleDir -Filter '*.msi' -File)
            }
            if ($candidatos.Count -gt 0) { $MsiPath = $candidatos[0].FullName }
        }
    }
    Assert-True ([bool]$MsiPath -and (Test-Path -LiteralPath $MsiPath -PathType Leaf)) "No se localizó el MSI candidato. Ruta evaluada: $MsiPath"

    $msi = $MsiPath
    $hash = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash
    $signature = (Get-AuthenticodeSignature -LiteralPath $msi).Status.ToString()
    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.OpenDatabase($msi, 0)
    $properties = @{}
    foreach ($row in (Read-MsiRows $database 'SELECT `Property`, `Value` FROM `Property`' 2)) { $properties[$row[0]] = $row[1] }
    $productCode = $properties['ProductCode']
    $tables = @((Read-MsiRows $database 'SELECT `Name` FROM `_Tables`' 1) | ForEach-Object { $_[0] })
    $files = @((Read-MsiRows $database 'SELECT `FileName` FROM `File`' 1) | ForEach-Object { $_[0] })
    $directories = @((Read-MsiRows $database 'SELECT `Directory`, `Directory_Parent`, `DefaultDir` FROM `Directory`' 3) | ForEach-Object { $_ -join ' | ' })
    $actions = @((Read-MsiRows $database 'SELECT `Action`, `Type`, `Source`, `Target` FROM `CustomAction`' 4) | ForEach-Object { $_ -join ' | ' })
    $sequence = @((Read-MsiRows $database 'SELECT `Action`, `Condition`, `Sequence` FROM `InstallExecuteSequence`' 3) | ForEach-Object { $_ -join ' | ' })
    $registry = @((Read-MsiRows $database 'SELECT `Root`, `Key`, `Name` FROM `Registry`' 3) | ForEach-Object { $_ -join ' | ' })
    $shortcuts = @((Read-MsiRows $database 'SELECT `Directory_`, `Name`, `Target` FROM `Shortcut`' 3) | ForEach-Object { $_ -join ' | ' })
    $report.msiCandidate = [ordered]@{
        file = (Get-Item -LiteralPath $msi).Name; bytes = (Get-Item -LiteralPath $msi).Length; sha256 = $hash; signature = $signature
        productName = $properties['ProductName']; productVersion = $properties['ProductVersion']
        productCode = $productCode; upgradeCode = $properties['UpgradeCode']; allUsers = $properties['ALLUSERS']
        files = $files; directories = $directories; customActions = $actions; executeSequence = $sequence
        registry = $registry; shortcuts = $shortcuts
        hasServiceInstall = ($tables -contains 'ServiceInstall')
    }

    Assert-True ($properties['ProductName'] -eq "Orchestrator's Brain") 'ProductName inesperado.'
    Assert-True ($properties['ProductVersion'] -eq '0.0.2') "ProductVersion inesperado; se esperaba 0.0.2 y se obtuvo $($properties['ProductVersion'])."
    Assert-True ($properties['ALLUSERS'] -eq '1') 'No se confirmó la instalación por máquina esperada.'
    Assert-True ($productCode -match '^\{[0-9A-Fa-f-]{36}\}$') 'ProductCode ausente o inválido.'
    Assert-True ($properties['UpgradeCode'] -eq '{50BDD96E-CF12-59A7-A2F3-980F1F41BC95}') 'UpgradeCode no coincide con la familia de producto Orchestrator''s Brain.'
    Assert-True (($files -match 'obrain-app\.exe').Count -eq 1) 'El MSI no declara obrain-app.exe exactamente una vez.'
    Assert-True (($files -match 'DirectML\.dll').Count -eq 1) 'El MSI no declara DirectML.dll exactamente una vez.'
    Assert-True (($directories -match 'ProgramFiles64Folder').Count -gt 0) 'No se vio ProgramFiles64Folder en Directory.'
    Assert-True ($tables -notcontains 'ServiceInstall') 'El MSI declara instalar un servicio no esperado.'
    Assert-True (-not (Get-UninstallEntry $productCode)) 'El producto ya está registrado antes del ensayo.'
    $report.preflight += 'Tablas MSI, versión 0.0.2, UpgradeCode, instalación por máquina y ausencia de servicio comprobados.'

    $report.webView2 = Get-WebView2Version
    $downloadsWebView = ($actions -match 'DownloadAndInvokeBootstrapper').Count -gt 0
    if ($downloadsWebView) {
        Assert-True (($sequence -match 'DownloadAndInvokeBootstrapper.*NOT\(REMOVE OR INSTALLED_WEBVIEW2_VERSION\)').Count -gt 0) 'Condición del bootstrapper WebView2 distinta de la esperada.'
        Assert-True ([bool]$report.webView2) 'WebView2 no consta instalado; el MSI podría descargar y ejecutar un bootstrapper externo. Instalación omitida.'
    }
    $report.preflight += "WebView2 registrado: $($report.webView2); bootstrapper condicional en MSI: $downloadsWebView."

    $cli = Join-Path $env:CARGO_TARGET_DIR 'release\obrain.exe'
    Assert-True (Test-Path -LiteralPath $cli -PathType Leaf) 'Falta la CLI compilada.'

    # =========================================================================
    # FASE 1: Instalación limpia, arranque y desinstalación de 0.0.2
    # =========================================================================
    $fixtureRoot = Join-Path $env:TEMP ('obrain-msi-' + [guid]::NewGuid().ToString('N'))
    $library = Join-Path $fixtureRoot 'biblioteca'
    & (Join-Path $PSScriptRoot 'biblioteca-sintetica.ps1') -Root $library -Fase Inicial | Out-Null
    Assert-True (@(Get-ChildItem -LiteralPath $library -Recurse -File).Count -eq 11) 'La biblioteca sintética no tiene 11 documentos.'
    $profile = Join-Path $fixtureRoot 'perfil'
    $appData = Join-Path $profile 'AppData\Roaming'
    $localAppData = Join-Path $profile 'AppData\Local'
    $temp = Join-Path $fixtureRoot 'temp'
    $webviewData = Join-Path $fixtureRoot 'webview2'
    foreach ($dir in @($profile, $appData, $localAppData, $temp, $webviewData)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    $db = Join-Path $profile '.obrain\brain.obrain'
    $env:OBRAIN_DB = $db
    $env:USERPROFILE = $profile
    $env:HOME = $profile
    $env:APPDATA = $appData
    $env:LOCALAPPDATA = $localAppData
    $env:TEMP = $temp
    $env:TMP = $temp
    $env:WEBVIEW2_USER_DATA_FOLDER = $webviewData

    & $cli --db $db --sin-modelo indexar $library *> $null
    Assert-True ($LASTEXITCODE -eq 0) "La preparación de la base sintética falló: $LASTEXITCODE"
    Assert-True (Test-Path -LiteralPath $db -PathType Leaf) 'No se creó la base sintética.'
    Assert-True (Test-Path -LiteralPath ($db + '.llave') -PathType Leaf) 'No se creó la llave de la base sintética.'

    $installLog = Join-Path $evidence 'msi-clean-install.log'
    $installAttempted = $true
    $installedProductCodesToCleanup += $productCode
    $installCode = Invoke-Msiexec '/i' $msi $installLog
    $report.cleanInstall.install = [ordered]@{ exitCode = $installCode; log = 'msi-clean-install.log' }
    Assert-True ($installCode -eq 0) "msiexec /i devolvió $installCode; no se asume instalación limpia completa."

    Start-Sleep -Seconds 1
    $postInstallInstaller = New-Object -ComObject WindowsInstaller.Installer
    $productState = [int]$postInstallInstaller.ProductState($productCode)
    $uninstallEntry = Get-UninstallEntry $productCode
    $installDir = Join-Path $env:ProgramFiles "Orchestrator's Brain"
    $exe = Join-Path $installDir 'obrain-app.exe'
    $directMl = Join-Path $installDir 'DirectML.dll'
    $exeExists = Test-Path -LiteralPath $exe -PathType Leaf
    $directMlExists = Test-Path -LiteralPath $directMl -PathType Leaf
    $appRegistry = 'HKCU:\Software\obrain\Orchestrator''s Brain'
    $registryValues = @{}
    foreach ($name in @('Desktop Shortcut', 'InstallDir', 'Uninstaller Shortcut', 'Start Menu Shortcut')) {
        $value = Get-ItemProperty -LiteralPath $appRegistry -Name $name -ErrorAction SilentlyContinue
        $registryValues[$name] = $(if ($value) { $value.$name } else { $null })
    }
    $report.cleanInstall.installed = [ordered]@{
        installDir = $installDir; exeExists = $exeExists
        exeBytes = $(if ($exeExists) { (Get-Item -LiteralPath $exe).Length } else { $null })
        exeSha256 = $(if ($exeExists) { (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash } else { $null })
        directMlExists = $directMlExists
        directMlBytes = $(if ($directMlExists) { (Get-Item -LiteralPath $directMl).Length } else { $null })
        productState = $productState; uninstallEntryFound = [bool]$uninstallEntry
        uninstallDisplayName = $(if ($uninstallEntry) { [string]$uninstallEntry.DisplayName } else { $null })
        uninstallDisplayVersion = $(if ($uninstallEntry) { [string]$uninstallEntry.DisplayVersion } else { $null })
        uninstallInstallLocation = $(if ($uninstallEntry) { [string]$uninstallEntry.InstallLocation } else { $null })
        appRegistryExists = (Test-Path -LiteralPath $appRegistry)
        appRegistryValues = $registryValues
    }
    Assert-True ($productState -eq 5) "Windows Installer no confirmó el producto instalado: ProductState=$productState."
    Assert-True ([bool]$uninstallEntry) 'Falta la entrada de desinstalación del producto instalado.'
    Assert-True ($report.cleanInstall.installed.uninstallDisplayName -eq $properties['ProductName']) 'El nombre de la entrada de desinstalación no coincide con el producto.'
    Assert-True ($report.cleanInstall.installed.uninstallDisplayVersion -eq '0.0.2') 'La versión de la entrada de desinstalación no es 0.0.2.'
    Assert-True (([string]$report.cleanInstall.installed.uninstallInstallLocation).TrimEnd([char]92) -eq $installDir) 'La ubicación registrada de desinstalación no coincide con INSTALLDIR.'
    Assert-True ($exeExists -and $directMlExists) 'Falta uno de los binarios declarados por el MSI.'
    Assert-True ($report.cleanInstall.installed.appRegistryExists) 'Falta la clave de registro HKCU declarada por el MSI.'
    Assert-True (([string]$registryValues['InstallDir']).TrimEnd([char]92) -eq $installDir) 'El valor HKCU InstallDir no coincide con INSTALLDIR.'

    $launched = $null
    $alive = $false
    if ($exeExists) {
        try {
            $launched = Start-Process -FilePath $exe -PassThru
            $sessionId = try { $launched.SessionId } catch { $null }
            Start-Sleep -Seconds 20
            $launched.Refresh()
            $alive = -not $launched.HasExited
            $report.cleanInstall.launch = [ordered]@{
                started = $true; processAliveAfter20Seconds = $alive
                exitCode = $(if ($alive) { $null } else { $launched.ExitCode })
                sessionId = $sessionId
                databaseExists = (Test-Path -LiteralPath $db -PathType Leaf)
                webViewProfileEntries = @(Get-ChildItem -LiteralPath $webviewData -Force -ErrorAction SilentlyContinue).Count
                originalProfileBrainExists = (Test-Path -LiteralPath $defaultDbDir)
                originalLocalAppDataBrainExists = (Test-Path -LiteralPath $oldDbDir)
            }
        } catch {
            $report.cleanInstall.launch = [ordered]@{ started = [bool]$launched; error = $_.Exception.Message }
        } finally {
            if ($launched) {
                $launched.Refresh()
                if (-not $launched.HasExited) { Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue }
            }
        }
    }
    Assert-True (-not (Test-Path -LiteralPath $defaultDbDir) -and -not (Test-Path -LiteralPath $oldDbDir)) 'El arranque escribió en la ubicación original de Brain del runner.'
    Assert-True ([bool]$launched -and $alive) 'La aplicación no arrancó y permaneció viva durante los 20 segundos esperados.'
    Assert-True ($report.cleanInstall.launch.databaseExists) 'La base sintética no sigue disponible después del arranque.'

    # Desinstalar candidato 0.0.2 para dejar el entorno listo para la prueba de actualización
    $uninstallLog = Join-Path $evidence 'msi-clean-uninstall.log'
    $uninstallCode = Invoke-Msiexec '/x' $productCode $uninstallLog
    $remainingEntry = [bool](Get-UninstallEntry $productCode)
    $remainingState = [int]$installer.ProductState($productCode)
    $remainingExe = Test-Path -LiteralPath $exe -PathType Leaf
    $remainingDirectMl = Test-Path -LiteralPath $directMl -PathType Leaf
    $appRegistryValueRemains = [bool](Get-ItemProperty -LiteralPath $appRegistry -Name InstallDir -ErrorAction SilentlyContinue)

    $report.cleanInstall.uninstall = [ordered]@{
        exitCode = $uninstallCode; log = 'msi-clean-uninstall.log'
        productState = $remainingState; uninstallRegistryRemains = $remainingEntry
        executableRemains = $remainingExe; directMlRemains = $remainingDirectMl
        appRegistryValueRemains = $appRegistryValueRemains
    }
    Assert-True ($uninstallCode -eq 0) "Desinstalación devolvió código $uninstallCode."
    Assert-True ($remainingState -eq -1) "ProductState post-desinstalación no es -1: $remainingState."
    Assert-True (-not $remainingEntry) 'Entrada de desinstalación subsiste tras msiexec /x.'
    Assert-True (-not $remainingExe -and -not $remainingDirectMl) 'Binarios persisten tras desinstalación.'
    $report.cleanInstall.status = 'completado'

    # =========================================================================
    # FASE 2: Actualización real desde versión 0.0.1 auténtica
    # =========================================================================
    if (-not $Msi001Path -and $env:OBRAIN_MSI_001_PATH) { $Msi001Path = $env:OBRAIN_MSI_001_PATH }
    if (-not $Msi001Path) {
        $posibles001 = @(
            (Join-Path $env:RUNNER_TEMP 'Orchestrator''s Brain_0.0.1_x64_en-US.msi'),
            (Join-Path $PSScriptRoot '..\tests\fixtures\msi-0.0.1\Orchestrator''s Brain_0.0.1_x64_en-US.msi')
        )
        foreach ($p in $posibles001) {
            if (Test-Path -LiteralPath $p -PathType Leaf) { $Msi001Path = $p; break }
        }
    }

    if ($Msi001Path -and (Test-Path -LiteralPath $Msi001Path -PathType Leaf)) {
        $hash001 = (Get-FileHash -LiteralPath $Msi001Path -Algorithm SHA256).Hash
        $hash001Autentico = '9AEE1B8125CC19DDBBF124350CD86EB510E77A9313C89094E0797A6AF7E35591'
        Assert-True ($hash001 -eq $hash001Autentico) "El MSI 0.0.1 no coincide con el paquete auténtico. SHA256=$hash001"

        $db001 = $installer.OpenDatabase($Msi001Path, 0)
        $props001 = @{}
        foreach ($row in (Read-MsiRows $db001 'SELECT `Property`, `Value` FROM `Property`' 2)) { $props001[$row[0]] = $row[1] }
        $productCode001 = $props001['ProductCode']
        Assert-True ($props001['ProductVersion'] -eq '0.0.1') "El paquete 0.0.1 declara versión $($props001['ProductVersion'])"
        Assert-True ($props001['UpgradeCode'] -eq $properties['UpgradeCode']) 'UpgradeCode no coincide entre 0.0.1 y 0.0.2'
        Assert-True ($productCode001 -ne $productCode) 'ProductCode no puede ser idéntico entre 0.0.1 y 0.0.2 en Major Upgrade'

        $report.msi001 = [ordered]@{
            ruta = $Msi001Path; sha256 = $hash001; productCode = $productCode001
            upgradeCode = $props001['UpgradeCode']; productVersion = $props001['ProductVersion']
        }

        # Preparar entorno aislado para la actualización
        $upgradeRoot = Join-Path $env:TEMP ('obrain-upgrade-' + [guid]::NewGuid().ToString('N'))
        $upgradeLib = Join-Path $upgradeRoot 'biblioteca'
        & (Join-Path $PSScriptRoot 'biblioteca-sintetica.ps1') -Root $upgradeLib -Fase Inicial | Out-Null
        $upgradeProfile = Join-Path $upgradeRoot 'perfil'
        $upgradeAppData = Join-Path $upgradeProfile 'AppData\Roaming'
        $upgradeLocalAppData = Join-Path $upgradeProfile 'AppData\Local'
        $upgradeTemp = Join-Path $upgradeRoot 'temp'
        $upgradeWebview = Join-Path $upgradeRoot 'webview2'
        foreach ($dir in @($upgradeProfile, $upgradeAppData, $upgradeLocalAppData, $upgradeTemp, $upgradeWebview)) {
            New-Item -ItemType Directory -Force -Path $dir | Out-Null
        }
        $upgradeDb = Join-Path $upgradeProfile '.obrain\brain.obrain'
        $env:OBRAIN_DB = $upgradeDb
        $env:USERPROFILE = $upgradeProfile
        $env:HOME = $upgradeProfile
        $env:APPDATA = $upgradeAppData
        $env:LOCALAPPDATA = $upgradeLocalAppData
        $env:TEMP = $upgradeTemp
        $env:TMP = $upgradeTemp
        $env:WEBVIEW2_USER_DATA_FOLDER = $upgradeWebview

        # Inicialización de la base sintética usando sólo el comportamiento indexador original de 0.0.1 (sin comandos de respaldo)
        & $cli --db $upgradeDb --sin-modelo indexar $upgradeLib *> $null
        Assert-True ($LASTEXITCODE -eq 0) "Indexación sintética para 0.0.1 falló: $LASTEXITCODE"
        & $cli --db $upgradeDb clave --guardada *> $null

        # 1. Instalar 0.0.1
        $log001Install = Join-Path $evidence 'msi001-install.log'
        $installedProductCodesToCleanup += $productCode001
        $code001 = Invoke-Msiexec '/i' $Msi001Path $log001Install
        $report.upgradeFrom001.install001 = [ordered]@{ exitCode = $code001; log = 'msi001-install.log' }
        Assert-True ($code001 -eq 0) "Instalación de 0.0.1 falló con código $code001."

        $post001Installer = New-Object -ComObject WindowsInstaller.Installer
        $state001 = [int]$post001Installer.ProductState($productCode001)
        Assert-True ($state001 -eq 5) "0.0.1 no figura instalado tras msiexec /i: ProductState=$state001"
        $report.upgradeFrom001.installed001 = [ordered]@{ productState = $state001; exeExists = (Test-Path -LiteralPath $exe) }

        # 2. Arranque preliminar de 0.0.1
        $proc001 = Start-Process -FilePath $exe -PassThru
        Start-Sleep -Seconds 10
        $proc001.Refresh()
        $alive001 = -not $proc001.HasExited
        if ($proc001 -and -not $proc001.HasExited) { Stop-Process -Id $proc001.Id -Force -ErrorAction SilentlyContinue }
        $report.upgradeFrom001.launch001 = [ordered]@{ started = $true; aliveAfter10s = $alive001 }
        Assert-True ($alive001) '0.0.1 no permaneció vivo durante el arranque inicial.'

        # 3. Actualizar a candidato 0.0.2 mediante msiexec /i
        $logUpgrade = Join-Path $evidence 'msi-upgrade-to-002.log'
        $installedProductCodesToCleanup += $productCode
        $upgradeCode = Invoke-Msiexec '/i' $msi $logUpgrade
        $report.upgradeFrom001.upgrade = [ordered]@{ exitCode = $upgradeCode; log = 'msi-upgrade-to-002.log' }
        Assert-True ($upgradeCode -eq 0) "Actualización a 0.0.2 falló con código $upgradeCode."

        Start-Sleep -Seconds 1
        $postUpgradeInstaller = New-Object -ComObject WindowsInstaller.Installer
        $state002AfterUpgrade = [int]$postUpgradeInstaller.ProductState($productCode)
        $state001AfterUpgrade = [int]$postUpgradeInstaller.ProductState($productCode001)
        $entry002 = Get-UninstallEntry $productCode
        $entry001 = Get-UninstallEntry $productCode001

        $report.upgradeFrom001.installed002 = [ordered]@{
            productState002 = $state002AfterUpgrade
            productState001 = $state001AfterUpgrade
            uninstall002Version = $(if ($entry002) { [string]$entry002.DisplayVersion } else { $null })
            uninstall001Remains = [bool]$entry001
            exeSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        }
        Assert-True ($state002AfterUpgrade -eq 5) "0.0.2 no figura instalado tras la actualización: ProductState=$state002AfterUpgrade."
        Assert-True ($state001AfterUpgrade -ne 5) "0.0.1 sigue figurando como producto instalado tras la actualización mayor."
        Assert-True ($report.upgradeFrom001.installed002.uninstall002Version -eq '0.0.2') 'La versión registrada de desinstalación post-actualización no es 0.0.2.'

        # 4. Arranque de la aplicación 0.0.2 tras la actualización
        $proc002 = Start-Process -FilePath $exe -PassThru
        Start-Sleep -Seconds 20
        $proc002.Refresh()
        $alive002 = -not $proc002.HasExited
        if ($proc002 -and -not $proc002.HasExited) { Stop-Process -Id $proc002.Id -Force -ErrorAction SilentlyContinue }
        $report.upgradeFrom001.launch002 = [ordered]@{ started = $true; aliveAfter20s = $alive002 }
        Assert-True ($alive002) '0.0.2 no permaneció vivo tras la actualización de versión.'

        # 5. Verificación de recuperación de conocimiento y relaciones post-actualización
        $libsPost = & $cli --db $upgradeDb bibliotecas
        $paquetesPost = & $cli --db $upgradeDb paquetes
        $sensiblesPost = & $cli --db $upgradeDb sensibles

        $hasLib = ($libsPost -match [regex]::Escape($upgradeLib)).Count -gt 0
        $hasAurora = ($paquetesPost -match 'expediente-AURORA\.txt').Count -gt 0
        $hasAnexo = ($paquetesPost -match 'anexo-cita\.txt').Count -gt 0
        $hasPasaporte = ($sensiblesPost -match 'Pasaporte de Lidia Robles\.txt').Count -gt 0

        $report.upgradeFrom001.knowledgeRecovery = [ordered]@{
            bibliotecaRecuperada = $hasLib
            paqueteAuroraRecuperado = $hasAurora
            anexoContenidoRecuperado = $hasAnexo
            sensibilidadRecuperada = $hasPasaporte
        }
        Assert-True ($hasLib) 'La biblioteca sintética no se recuperó tras la actualización.'
        Assert-True ($hasAurora -and $hasAnexo) 'La relación de contención (part_of) no se recuperó tras la actualización.'
        Assert-True ($hasPasaporte) 'La marca de sensibilidad no se recuperó tras la actualización.'

        # 6. Desinstalación final de 0.0.2
        $logFinalUninstall = Join-Path $evidence 'msi-final-uninstall.log'
        $finalUninstallCode = Invoke-Msiexec '/x' $productCode $logFinalUninstall
        $report.upgradeFrom001.uninstall002 = [ordered]@{ exitCode = $finalUninstallCode; log = 'msi-final-uninstall.log' }
        Assert-True ($finalUninstallCode -eq 0) "Desinstalación final falló con código $finalUninstallCode."
        $report.upgradeFrom001.status = 'completado'
    } else {
        $report.upgradeFrom001.status = 'pendiente'
        $report.upgradeFrom001.reason = 'Paquete MSI 0.0.1 auténtico no suministrado en el entorno de ejecución.'
    }
} catch {
    $failure = $_
    $report.error = $_.Exception.Message
} finally {
    # Limpieza de salvaguarda en el runner
    foreach ($codeToClean in $installedProductCodesToCleanup) {
        try {
            $entry = Get-UninstallEntry $codeToClean
            if ($entry) {
                Invoke-Msiexec '/x' $codeToClean (Join-Path $evidence "cleanup-$codeToClean.log") | Out-Null
            }
        } catch {}
    }

    if (-not $report.error -and $failure) { $report.error = [string]$failure }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence 'resultado.json') -Encoding utf8
    if ($failure) { throw $failure }
}
