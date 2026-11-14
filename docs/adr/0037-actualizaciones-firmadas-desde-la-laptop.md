# ADR-0037 — Actualizaciones firmadas, publicadas desde la laptop

- Estado: implementada en `feat/actualizaciones`; pendiente de la primera publicación real.
- Fecha: 2026-11-13.

## Problema

Hasta la 0.0.2, cada cambio pedía compilar el MSI e instalarlo a mano. Con la
bóveda real en uso van a salir muchos ajustes pequeños, y reinstalar por cada
uno hace que se queden sin llegar. JP pidió lo que ya tiene `job-rob`: publicar
a propósito y que la aplicación se ponga al día al abrirla.

`job-rob` es Python y se actualiza trayendo la última etiqueta de git. Brain
es un programa compilado: lo que viaja es un instalador, y un instalador que
se ejecuta solo es código que alguien ajeno podría intentar colar. Esta
aplicación guarda el índice de documentos de identidad y de dinero.

## Decisión

**Al abrir, Brain pregunta; instalar lo decide la persona.** Con la ventana ya
abierta, pide a GitHub el archivo `latest.json` de la última versión publicada.
Si es más nueva que la instalada, el pie de la barra lateral ofrece «Versión X
· Actualizar». Al pulsar, descarga el MSI, comprueba su firma y lo lanza; el
instalador cierra la aplicación y la vuelve a abrir.

**Sólo se instala lo que firma la llave de publicación.** La llave pública va
compilada en la aplicación (`plugins.updater.pubkey`). La privada vive en
`%USERPROFILE%\.obrain-publicar`, fuera del repositorio y de OneDrive, con una
contraseña que se teclea en cada publicación. No está en GitHub: quien entre
en la cuenta puede subir archivos, pero no firmarlos, y la aplicación descarta
lo que no venga firmado.

**La firma dice para qué versión se hizo** (`requireSignedVersion`). El
`latest.json` no va firmado; sin esto, alguien capaz de servirlo podría
anunciar un número alto con el instalador —auténtico y bien firmado— de una
versión vieja y con fallos conocidos.

**Se publica desde la laptop, a propósito** (`tools/publicar.ps1`). Sólo desde
`main`, sin cambios pendientes y ya subido; compila, firma, comprueba la firma
y, sólo al final, crea la etiqueta `vX.Y.Z` y la versión en GitHub. Un commit
en `main` no llega a nadie: llega lo que se publica.

**Si algo falla, no pasa nada.** Sin red o con GitHub caído no hay aviso ni
error: la consulta va después de abrir la bóveda y no la espera. En desarrollo
(`npm run app`) y en una copia compilada sin llave pública, no se consulta.

**La ventana no gana permisos.** Buscar e instalar son dos comandos de Rust
sin argumentos; el webview no recibe permisos del plugin ni puede elegir
dirección o versión. La política de contenido de la ventana no cambia.

## Lo que cambia en la promesa de «nada sale de tu computadora»

Brain pasa a hacer **una conexión de red al abrirse**: una petición a
`github.com` que no lleva nada de la bóveda. GitHub ve la dirección IP y que
alguien pidió ese archivo. La otra conexión que ya existía es la descarga del
modelo, que se pide a mano.

## Límites conocidos

- **El instalador no lleva firma de Windows** (Authenticode). La firma de este
  ADR la comprueba Brain, no Windows.
- **Perder la llave privada** deja las copias instaladas sin poder actualizarse
  solas: hay que crear otra llave e instalar a mano una vez. Por eso se guarda
  una copia fuera de la laptop.
- **Quien robe la llave y su contraseña, y además pueda publicar en el
  repositorio**, puede mandar una actualización. Hacen falta las dos cosas.
- La comprobación previa a instalar (no hay restauración ni lectura de la
  bóveda en marcha) no cierra la carrera con el vigía: si una lectura empieza
  justo después, el instalador la corta como un cierre brusco, que SQLite
  resuelve al volver a abrir.
- La 0.0.2 y anteriores no tienen actualizador: la primera versión con él se
  instala a mano.
- `tools/validar-msi-runner.ps1` sigue escrito para el candidato 0.0.2.

## Cómo se publica

```powershell
./tools/crear-llave-publicacion.ps1                      # una sola vez
./tools/publicar.ps1 -Version 0.0.4 -Notas "Qué cambia"  # cada versión
./tools/publicar.ps1 -SoloMirar                          # dice qué haría
```

## Verificación

- `ui/tests/actualizacion-ui.test.mjs`: la ventana ofrece sin instalar, calla
  los fallos de red, no manda dirección ni versión, no instala dos veces y
  conserva el aviso si la firma no vale.
- No probado todavía: una actualización real de extremo a extremo entre dos
  versiones publicadas, que sólo puede hacerse tras publicar la segunda.
