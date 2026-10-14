# Ejecutar sólo en el runner Windows temporal del workflow.
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
    msi = $null
    preflight = @()
    install = $null
    installed = $null
    launch = $null
    uninstall = $null
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
        while ($record = $view.Fetch()) {
            $row = @()
            for ($i = 1; $i -le $columns; $i++) { $row += [string]$record.StringData($i) }
            [pscustomobject]@{ Columns = $row }
        }
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

    $msis = @(Get-ChildItem -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'release\bundle\msi') -Filter '*.msi' -File)
    Assert-True ($msis.Count -eq 1) "Se esperaba exactamente un MSI compilado, se encontraron $($msis.Count)."
    $msi = $msis[0].FullName
    $hash = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash
    $signature = (Get-AuthenticodeSignature -LiteralPath $msi).Status.ToString()
    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.OpenDatabase($msi, 0)
    $properties = @{}
    foreach ($row in (Read-MsiRows $database 'SELECT `Property`, `Value` FROM `Property`' 2)) { $properties[$row.Columns[0]] = $row.Columns[1] }
    $productCode = $properties['ProductCode']
    $tables = @((Read-MsiRows $database 'SELECT `Name` FROM `_Tables`' 1) | ForEach-Object { $_.Columns[0] })
    $files = @((Read-MsiRows $database 'SELECT `FileName` FROM `File`' 1) | ForEach-Object { $_.Columns[0] })
    $directories = @((Read-MsiRows $database 'SELECT `Directory`, `Directory_Parent`, `DefaultDir` FROM `Directory`' 3) | ForEach-Object { $_.Columns -join ' | ' })
    $actions = @((Read-MsiRows $database 'SELECT `Action`, `Type`, `Source`, `Target` FROM `CustomAction`' 4) | ForEach-Object { $_.Columns -join ' | ' })
    $sequence = @((Read-MsiRows $database 'SELECT `Action`, `Condition`, `Sequence` FROM `InstallExecuteSequence`' 3) | ForEach-Object { $_.Columns -join ' | ' })
    $registry = @((Read-MsiRows $database 'SELECT `Root`, `Key`, `Name` FROM `Registry`' 3) | ForEach-Object { $_.Columns -join ' | ' })
    $shortcuts = @((Read-MsiRows $database 'SELECT `Directory_`, `Name`, `Target` FROM `Shortcut`' 3) | ForEach-Object { $_.Columns -join ' | ' })
    $report.msi = [ordered]@{
        file = $msis[0].Name; bytes = $msis[0].Length; sha256 = $hash; signature = $signature
        productName = $properties['ProductName']; productVersion = $properties['ProductVersion']
        productCode = $productCode; allUsers = $properties['ALLUSERS']
        files = $files; directories = $directories; customActions = $actions; executeSequence = $sequence
        registry = $registry; shortcuts = $shortcuts
        hasServiceInstall = ($tables -contains 'ServiceInstall')
    }
    Assert-True ($properties['ProductName'] -eq "Orchestrator's Brain") 'ProductName inesperado.'
    Assert-True ($properties['ALLUSERS'] -eq '1') 'No se confirmó la instalación por máquina esperada.'
    Assert-True ($productCode -match '^\{[0-9A-Fa-f-]{36}\}$') 'ProductCode ausente o inválido.'
    Assert-True (($files -match 'obrain-app\.exe').Count -eq 1) 'El MSI no declara obrain-app.exe exactamente una vez.'
    Assert-True (($files -match 'DirectML\.dll').Count -eq 1) 'El MSI no declara DirectML.dll exactamente una vez.'
    Assert-True (($directories -match 'ProgramFiles64Folder').Count -gt 0) 'No se vio ProgramFiles64Folder en Directory.'
    Assert-True ($tables -notcontains 'ServiceInstall') 'El MSI declara instalar un servicio no esperado.'
    Assert-True (-not (Get-UninstallEntry $productCode)) 'El producto ya está registrado antes del ensayo.'
    $report.preflight += 'Tablas MSI, carga, instalación por máquina, producto y ausencia de servicio inspeccionados.'

    $report.webView2 = Get-WebView2Version
    $downloadsWebView = ($actions -match 'DownloadAndInvokeBootstrapper').Count -gt 0
    if ($downloadsWebView) {
        Assert-True (($sequence -match 'DownloadAndInvokeBootstrapper.*NOT\(REMOVE OR INSTALLED_WEBVIEW2_VERSION\)').Count -gt 0) 'Condición del bootstrapper WebView2 distinta de la esperada.'
        Assert-True ([bool]$report.webView2) 'WebView2 no consta instalado; el MSI podría descargar y ejecutar un bootstrapper externo. Instalación omitida.'
    }
    $report.preflight += "WebView2 registrado: $($report.webView2); bootstrapper condicional en MSI: $downloadsWebView."

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
    $cli = Join-Path $env:CARGO_TARGET_DIR 'release\obrain.exe'
    Assert-True (Test-Path -LiteralPath $cli -PathType Leaf) 'Falta la CLI compilada.'
    & $cli --db $db --sin-modelo indexar $library *> $null
    Assert-True ($LASTEXITCODE -eq 0) "La preparación de la base sintética falló: $LASTEXITCODE"
    Assert-True (Test-Path -LiteralPath $db -PathType Leaf) 'No se creó la base sintética.'
    Assert-True (Test-Path -LiteralPath ($db + '.llave') -PathType Leaf) 'No se creó la llave de la base sintética.'
    $report.preflight += 'Base cifrada creada con la CLI y sólo 11 documentos de la fixture; salida suprimida para no registrar la clave de recuperación.'

    $installLog = Join-Path $evidence 'msi-install.log'
    $installAttempted = $true
    $installCode = Invoke-Msiexec '/i' $msi $installLog
    $report.install = [ordered]@{ exitCode = $installCode; log = 'msi-install.log' }
    Assert-True ($installCode -eq 0) "msiexec /i devolvió $installCode; no se asume instalación completa."

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
    $report.installed = [ordered]@{
        installDir = $installDir
        exeExists = $exeExists
        exeBytes = $(if ($exeExists) { (Get-Item -LiteralPath $exe).Length } else { $null })
        exeSha256 = $(if ($exeExists) { (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash } else { $null })
        directMlExists = $directMlExists
        directMlBytes = $(if ($directMlExists) { (Get-Item -LiteralPath $directMl).Length } else { $null })
        productState = $productState
        uninstallEntryFound = [bool]$uninstallEntry
        uninstallDisplayName = $(if ($uninstallEntry) { [string]$uninstallEntry.DisplayName } else { $null })
        uninstallInstallLocation = $(if ($uninstallEntry) { [string]$uninstallEntry.InstallLocation } else { $null })
        appRegistryExists = (Test-Path -LiteralPath $appRegistry)
        appRegistryValues = $registryValues
    }
    Assert-True ($productState -eq 5) "Windows Installer no confirmó el producto instalado: ProductState=$productState."
    Assert-True ([bool]$uninstallEntry) 'Falta la entrada de desinstalación del producto instalado.'
    Assert-True ($report.installed.uninstallDisplayName -eq $properties['ProductName']) 'El nombre de la entrada de desinstalación no coincide con el producto.'
    Assert-True (([string]$report.installed.uninstallInstallLocation).TrimEnd([char]92) -eq $installDir) 'La ubicación registrada de desinstalación no coincide con INSTALLDIR.'
    Assert-True ($exeExists -and $directMlExists) 'Falta uno de los binarios declarados por el MSI.'
    Assert-True ($report.installed.appRegistryExists) 'Falta la clave de registro HKCU declarada por el MSI.'
    Assert-True (([string]$registryValues['InstallDir']).TrimEnd([char]92) -eq $installDir) 'El valor HKCU InstallDir no coincide con INSTALLDIR.'
    # El runner puede carecer de escritorio interactivo. Esto sólo prueba un arranque de proceso.
    $launched = $null
    $alive = $false
    if ($exeExists) { try {
        $launched = Start-Process -FilePath $exe -PassThru
        $sessionId = try { $launched.SessionId } catch { $null }
        Start-Sleep -Seconds 20
        $launched.Refresh()
        $alive = -not $launched.HasExited
        $report.launch = [ordered]@{
            started = $true; processAliveAfter20Seconds = $alive
            exitCode = $(if ($alive) { $null } else { $launched.ExitCode })
            sessionId = $sessionId
            databaseExists = (Test-Path -LiteralPath $db -PathType Leaf)
            webViewProfileEntries = @(Get-ChildItem -LiteralPath $webviewData -Force -ErrorAction SilentlyContinue).Count
            originalProfileBrainExists = (Test-Path -LiteralPath $defaultDbDir)
            originalLocalAppDataBrainExists = (Test-Path -LiteralPath $oldDbDir)
        }
    } catch {
        $report.launch = [ordered]@{ started = [bool]$launched; error = $_.Exception.Message }
    } finally {
        if ($launched) {
            $launched.Refresh()
            if (-not $launched.HasExited) { Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue }
        }
    }} else { $report.launch = [ordered]@{ started = $false; reason = 'No existe el ejecutable instalado.' } }
    Assert-True (-not (Test-Path -LiteralPath $defaultDbDir) -and -not (Test-Path -LiteralPath $oldDbDir)) 'El arranque escribió en la ubicación original de Brain del runner.'
    Assert-True ([bool]$launched -and $alive) 'La aplicación no arrancó y permaneció viva durante los 20 segundos esperados.'
    Assert-True ($report.launch.databaseExists) 'La base sintética no sigue disponible después del arranque.'
} catch {
    $failure = $_
    $report.error = $_.Exception.Message
} finally {
    if ($installAttempted -and $productCode) {
        try {
            $uninstallLog = Join-Path $evidence 'msi-uninstall.log'
            $uninstallCode = Invoke-Msiexec '/x' $productCode $uninstallLog
            $remainingEntry = [bool](Get-UninstallEntry $productCode)
            $remainingState = [int]$installer.ProductState($productCode)
            $remainingExe = $false
            $remainingDirectMl = $false
            if ($report.installed) {
                $remainingExe = Test-Path -LiteralPath (Join-Path $report.installed.installDir 'obrain-app.exe')
                $remainingDirectMl = Test-Path -LiteralPath (Join-Path $report.installed.installDir 'DirectML.dll')
            }
            $appRegistryValueRemains = [bool](Get-ItemProperty -LiteralPath 'HKCU:\Software\obrain\Orchestrator''s Brain' -Name InstallDir -ErrorAction SilentlyContinue)
            $report.uninstall = [ordered]@{
                exitCode = $uninstallCode; log = 'msi-uninstall.log'
                productState = $remainingState; uninstallRegistryRemains = $remainingEntry
                executableRemains = $remainingExe; directMlRemains = $remainingDirectMl
                appRegistryValueRemains = $appRegistryValueRemains
            }
            if ($uninstallCode -ne 0 -or $remainingState -ne -1 -or $remainingEntry -or $remainingExe -or $remainingDirectMl -or $appRegistryValueRemains) {
                if (-not $failure) { $failure = 'La desinstalación no se confirmó.' }
            }
        } catch {
            $report.uninstall = [ordered]@{ error = $_.Exception.Message }
            if (-not $failure) { $failure = $_ }
        }
    }
    if (-not $report.error -and $failure) { $report.error = [string]$failure }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence 'resultado.json') -Encoding utf8
    if ($failure) { throw $failure }
}
