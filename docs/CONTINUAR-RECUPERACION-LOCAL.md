# Continuar la evaluación en local

Entrega del 15 de octubre de 2026, rama `work`, sobre `a962e75` (PR #10).
Empieza por [ESTADO](ESTADO.md), [EVALUACIÓN](EVALUACION-RECUPERACION.md) y el
[registro de preparación](evaluaciones/preparacion-e5-2026-10-15.json).
No uses los prompts históricos de `docs/archivo/` para elegir rama.

## Traer esta entrega

Desde tu checkout local, comprueba primero `git status --short` y conserva
cualquier cambio propio. Con el árbol local preparado para cambiar de rama:

```powershell
git fetch origin
git switch work
git pull --ff-only origin work
git status --short
```

Si aún no existe la rama local y `git switch work` no la crea automáticamente,
usa `git switch --track origin/work`. Si hay divergencia, conserva ambos
historiales y revísala; no uses reset ni push forzado.

## Qué está medido

- BM25: documento top 1, documento top 3 y cita suficiente en **11/14**.
- Repetición aprobada (Cargo 0), sin cambios de candidatos, orden o citas en
  las 17 preguntas. Literalidad, navegación por versión y PIN comprobados.
- E5 **sin medir**: sólo se descargó/verificó `config.json`. El intento E5
  rechazó el directorio incompleto (Cargo 101), sin informe híbrido.
- Cloud: `huggingface.co` responde, pero el CDN `us.aws.cdn.hf.co` devuelve
  403 en el túnel del proxy. Sólo había herramienta de consulta del entorno.
  Para continuar allí, añadir ese CDN conservando `huggingface.co` y
  `package_managers`, guardar/aplicar y verificar acceso observado.

Los pesos, el runtime y `/workspace/.obrain-env/env.sh` pertenecen al entorno
Linux y no viajan en Git. En local usa tu toolchain Rust/ONNX Runtime para
Windows; su compilación y ejecución siguen pendientes en esta entrega. El
README y ESTADO documentan los requisitos de Perl/OpenSSL y el directorio
de compilación externo si la ruta del repo contiene un apóstrofo.

## Descargar la revisión fija en Windows

Sólo el modelo oficial, fuera del checkout y de la bóveda personal. No uses
el descargador genérico para esta comparación: resuelve `main`, mientras que
este experimento exige la revisión fija. En PowerShell:

```powershell
$ErrorActionPreference = 'Stop'
$revision = '614241f622f53c4eeff9890bdc4f31cfecc418b3'
$modelDir = Join-Path $env:USERPROFILE ".obrain-eval\models\multilingual-e5-small\$revision"
New-Item -ItemType Directory -Force -Path $modelDir | Out-Null
$baseUrl = "https://huggingface.co/intfloat/multilingual-e5-small/resolve/$revision"
$artifacts = @(
    @{ File='model.onnx'; Remote='onnx/model.onnx'; Hash='ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665' },
    @{ File='tokenizer.json'; Remote='tokenizer.json'; Hash='0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39' },
    @{ File='config.json'; Remote='config.json'; Hash='69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959' }
)
foreach ($artifact in $artifacts) {
    $dest = Join-Path $modelDir $artifact.File
    curl.exe --fail --location --output "$dest.part" "$baseUrl/$($artifact.Remote)"
    if ($LASTEXITCODE -ne 0) { throw "Descarga fallida: $($artifact.File)" }
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath "$dest.part").Hash.ToLowerInvariant()
    if ($hash -ne $artifact.Hash) { throw "Hash incorrecto: $($artifact.File)" }
    Move-Item -Force -LiteralPath "$dest.part" -Destination $dest
}
```

Mantén proxy y TLS. Si la red local bloquea una redirección, registra el
destino y resuelve el permiso mediante su configuración. Los hashes LFS
esperados están reconfirmados en metadatos oficiales; sólo un hash calculado
tras descargar acredita el archivo local. Linux: comandos equivalentes en
[EVALUACIÓN](EVALUACION-RECUPERACION.md).

## Ejecutar el banco y decidir el siguiente cambio

Desde la raíz del repo, con el runtime local configurado y las variables del
bloque anterior disponibles. Informes y logs van a un directorio temporal nuevo:

```powershell
$ErrorActionPreference = 'Continue'
$reports = Join-Path $env:TEMP ("obrain-e5-eval-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $reports -ErrorAction Stop | Out-Null
$env:OBRAIN_EVAL_REPORT = Join-Path $reports 'lexical.json'
cargo test --manifest-path core/Cargo.toml --locked -p obrain-api --test recuperacion_banco evaluar_recuperacion_lexica -- --exact --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $reports 'lexical-cargo.log')
$lexicalExit = $LASTEXITCODE
$lexicalExit | Set-Content -Path (Join-Path $reports 'lexical-cargo.exit')
if ($lexicalExit -ne 0) { throw 'Falló el banco léxico' }
$env:OBRAIN_EVAL_MODEL = $modelDir
$env:OBRAIN_EVAL_REPORT = Join-Path $reports 'hybrid.json'
cargo test --manifest-path core/Cargo.toml --locked -p obrain-api --test recuperacion_banco evaluar_hibrida_e5 -- --ignored --exact --nocapture 2>&1 | Tee-Object -FilePath (Join-Path $reports 'hybrid-cargo.log')
$hybridExit = $LASTEXITCODE
$hybridExit | Set-Content -Path (Join-Path $reports 'hybrid-cargo.exit')
if ($hybridExit -ne 0) { throw 'Falló E5: conservar log; no atribuirle métricas aprobadas' }
Write-Output "Informes y salidas: $reports"
```

No cambiar preguntas, respuestas ni criterios. Guardar revisión, hashes
observados, informes completos y salida/código de Cargo. Comparar documento
top 1, top 3 y cita suficiente por separado; revisar las tres paráfrasis,
las dos consultas exactas y los once casos léxicos, incluyendo cambios de orden.
Mascotas, IBAN y teléfono siguen siendo negativos: relación temática no es
respuesta. El informe identifica la integración como `integration-1`; la
revisión real y hashes deben acompañarlo en el registro de ejecución.

Documento ausente → recuperación/fusión. Documento correcto con cita
insuficiente → selección de pasajes. Ambas capas suficientes → experimento
acotado de respuesta con citas y abstención. Actualizar los cinco registros
solicitados en el traspaso. Sólo fixtures y bases temporales; no abrir ni
migrar la bóveda personal, ni avanzar a conversación o memoria general.
La validación visual Windows sigue pendiente. No fusionar ni desplegar.
