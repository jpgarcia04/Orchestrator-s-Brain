param(
    [Parameter(Mandatory = $true)][string]$Root,
    [ValidateSet('Inicial', 'Cambio')][string]$Fase = 'Inicial'
)

$ErrorActionPreference = 'Stop'
$origen = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\tests\fixtures\producto-windows'))
$destino = [IO.Path]::GetFullPath($Root)
$temp = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')
if (-not [IO.Path]::IsPathRooted($Root) -or $Root -match '^[A-Za-z]:[^\\/]') {
    throw 'Root debe ser una ruta absoluta'
}
if (-not $destino.StartsWith($temp + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "Root debe quedar dentro de TEMP: $temp"
}
if ($destino.StartsWith($origen + '\', [StringComparison]::OrdinalIgnoreCase) -or
    $destino.Equals($origen, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'La copia no puede vivir dentro de la fixture original'
}

if ($Fase -eq 'Inicial') {
    if (Test-Path -LiteralPath $destino) { throw "La raíz ya existe: $destino" }
    New-Item -ItemType Directory -Path $destino | Out-Null
    foreach ($carpeta in (Get-ChildItem -LiteralPath $origen -Directory)) {
        Copy-Item -LiteralPath $carpeta.FullName -Destination $destino -Recurse
    }
    $archivos = @(Get-ChildItem -LiteralPath $destino -Recurse -File)
    if ($archivos.Count -ne 11) { throw "Se esperaban 11 documentos, hay $($archivos.Count)" }
    Write-Output "Biblioteca sintética creada: $destino ($($archivos.Count) documentos)"
    return
}

if (-not (Test-Path -LiteralPath $destino -PathType Container)) { throw 'Primero ejecuta la fase Inicial' }
if ((Get-Item -LiteralPath $destino).Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) {
    throw 'Root no puede ser un enlace ni punto de reanálisis'
}

$apuntes = Join-Path $destino '01 estudios\apuntes-redes.md'
$recibo = Join-Path $destino '04 finanzas\recibo-septiembre-copia.txt'
$contrato = Join-Path $destino '03 contratos\Contrato alquiler ficticio.txt'
foreach ($par in @(@($apuntes, '01 estudios\apuntes-redes.md'),
                    @($recibo, '04 finanzas\recibo-septiembre-copia.txt'),
                    @($contrato, '03 contratos\Contrato alquiler ficticio.txt'))) {
    $base = Join-Path $origen $par[1]
    if (-not (Test-Path -LiteralPath $par[0] -PathType Leaf) -or
        (Get-FileHash -LiteralPath $par[0] -Algorithm SHA256).Hash -ne
        (Get-FileHash -LiteralPath $base -Algorithm SHA256).Hash) {
        throw "El documento inicial cambió o falta: $($par[0])"
    }
}

$nuevoContrato = Join-Path $destino '02 tramites\Contrato alquiler revisado.txt'
if (Test-Path -LiteralPath $nuevoContrato) { throw 'El contrato de destino ya existe' }
Add-Content -LiteralPath $apuntes -Encoding utf8 -Value "`nLa revisión posterior compara una topología de respaldo con el trazado inicial."
Add-Content -LiteralPath $recibo -Encoding utf8 -Value "`nEsta copia incorpora una anotación posterior y ya no es idéntica al recibo original."
Move-Item -LiteralPath $contrato -Destination $nuevoContrato
Write-Output "Cambio aplicado: contenido de apuntes y copia actualizado; contrato movido y renombrado"
