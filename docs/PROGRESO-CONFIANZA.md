# Progreso — 2026-10-11

## Alcance y decisiones

Checkout inicial `c183f3c`, limpio, en la rama de desarrollo existente.
Trabajo en `codex/brain-confianza`, sin reset ni descarte de cambios. No había
un PR abierto apropiado. Se conserva la arquitectura y no se añaden integraciones.

Una entrega de la primera etapa con commits separados para núcleo/puente,
cliente y documentación. La base del PR es
`claude/orchestrators-brain-fase-3-wdiheq`: contiene el producto auditado; apuntar
directamente a `main` mezclaría el desarrollo previo con esta corrección.

Cambios: lista cerrada de correcciones y objetos tipados; confirmación por ID;
autorización de sujeto/destino y comandos alternativos; savepoints y proyección;
URL limitada al origen; filtrado de metadatos antes de serializar; apertura
sólo por ruta determinista con cerrojo; plazo fijo del PIN y revocación local;
descarte de respuestas antiguas y retirada de caches/claves. Purga de vectores
`vault` sin modelo y filtro en kNN incluidos como dependencia de seguridad.

La revisión detectó otra frontera: dos decisiones de sensibilidad con el mismo
timestamp podían mantener vigente la anterior. La decisión del usuario ahora
es única, aunque las señales del clasificador sigan siendo múltiples.

## Evidencia antes/después

Las primeras cinco pruebas de `confianza.rs`, sobre el código anterior,
fallaron por aceptación de ruta interna, tipo incorrecto, salto del PIN,
metadatos protegidos serializados y sensibilidad que permanecía normal.
Pasan con el arreglo. La confirmación de relaciones conserva el ID del objeto,
es idempotente y rechaza ID ajeno/retractado; no se borran inferencias.

Dos regresiones compilan y ejecutan el estado real de Svelte con transporte
sintético: el estado de `c183f3c` concede acceso tras fallar seguridad y no
revoca antes de esperar al bloqueo; ambas fallan en modo histórico y pasan
con el estado nuevo. El resto comprueba caducidad, contenido recibido, petición
y consulta de seguridad en vuelo, bloqueo fallido/pendiente, desbloqueo posterior
y lectura legítima sin PIN.

La comparación adicional usa una copia temporal de `git archive c183f3c`, sin
abrir datos persistentes: misma regresión de decisiones simultáneas, origen
guardado en un flujo NTFS sintético y vector `vault` sin purgar.
Las tres fallan sobre el código histórico por el motivo esperado y pasan
sobre el código corregido. La comparación compartió inicialmente artefactos
de compilación y produjo un error de resolución de crate en un doctest;
se repitió la validación actual sin compilaciones concurrentes. En futuras
comparaciones usar un `CARGO_TARGET_DIR` separado para cada checkout.

Reproducción de las dos regresiones UI históricas desde `ui/` (Node 24):

```powershell
npm test
$env:OBRAIN_BASELINE = '1'
node --test tests/app-security.test.mjs  # se esperan dos fallos
Remove-Item Env:OBRAIN_BASELINE
```

## Validación y límites

Validación final sobre el código entregado, con lockfiles y en Windows:

| Comprobación | Resultado |
|---|---|
| `cargo test --locked --workspace` desde `core/` | 487 pruebas y 1 doctest aprobados |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Aprobado |
| `cargo fmt --check` en ambos workspaces | Aprobado |
| `npm test` desde `ui/`, Node 24 | 9 pruebas aprobadas |
| `npm run check` | 0 errores y 0 advertencias |
| `npm run build` | Aprobado |
| `cargo check --locked --manifest-path ../ui/src-tauri/Cargo.toml` desde `core/` | Aprobado; workspace independiente |
| `git diff --check` | Aprobado |

Se usó un directorio de compilación fuera del checkout por OpenSSL en Windows.
El enlazador emitió avisos de símbolos PDB ausentes en dependencias nativas;
las pruebas y compilaciones terminaron correctamente.

Los tests Rust usan memoria o carpetas temporales; el caso NTFS crea y elimina
un archivo sintético.
No se ejecutó la app, no se abrió la bóveda real, no se reindexó, migró ni saneó
información personal. No se verificó una restauración de la bóveda real.

No se ha probado la interacción visual de la app empaquetada, OCR documental
real, calidad E5 sobre corpus representativo, batería ni bibliotecas grandes.
Persisten los dos paquetes npm con avisos altos; el alcance inspeccionado está
en `AUDITORIA-CONFIANZA.md`. No se aplicó una actualización de dependencias npm.

La política histórica de retención queda preparada como propuesta y procedimiento;
no se pide aprobación para ella en esta entrega porque no se ejecuta ninguna
operación sobre datos existentes. No hay cambio de esquema ni de criptografía.

## Siguiente sesión

Revisar el PR, después identidad entre bibliotecas y publicación tras
clasificación. Continuar con relaciones obsoletas, completitud de búsqueda y
persistencia/recuperación. Retener la visión de Jarvis mediante mejoras pequeñas
de utilidad comprobable y memoria explícita ligada a fuentes.
