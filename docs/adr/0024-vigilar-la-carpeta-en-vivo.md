# ADR-0024 — Vigilar la carpeta en vivo: todo otra vez, tras tres segundos de calma

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Decide:** lo que faltaba de M5c («lo nuevo aparece sin lanzar nada») y la Fase 3.5 del maestro (§9)
- **Depende de:** [ADR-0018](0018-bibliotecas.md), [ADR-0023](0023-vencimientos.md)

## Contexto

Desde la Fase 1 la app se pone al día **al abrirse** y con **«actualizar»**:
`indexar::reindex` vuelve a leer todas las bibliotecas, en un hilo y con su
propia conexión, y es idempotente —lo que no cambió no se reescribe—. Lo que
falta es enterarse **mientras está abierta**: guardas la foto de la TIE en
`01 Identidad` y no aparece hasta que pulsas «actualizar».

El maestro (§4.1) ya decía la vía: `ReadDirectoryChangesW` para carpetas
concretas (el USN Change Journal, para volúmenes enteros, está congelado). El
crate `notify` la usa en Windows e `inotify` en Linux, que es lo que permite
probarlo en el banco de pruebas.

Se le enseñaron al usuario las decisiones con coste antes del código, y eligió
(2026-09-23) la recomendada en las cuatro:

| Pregunta | Eligió |
|---|---|
| ¿Qué se vuelve a leer? | **Todo**, como «actualizar», tras una pausa |
| ¿Cuánto se espera? | **3 segundos** desde el último cambio |
| ¿Y con la app cerrada? | **Nada**: se pone al día al abrirla, como hoy |
| ¿Cómo se entera? | **El indicador de siempre**: «Leyendo tu bóveda…» → «Al día» |

## Decisión

1. **Un vigía por aplicación** (`obrain_api::vigilar::Vigia`) con `notify`,
   recursivo sobre cada biblioteca registrada. Se sincroniza al terminar cada
   pasada: una carpeta añadida con «Añadir carpeta» empieza a vigilarse sola, y
   una biblioteca que ya no está en el disco se deja de vigilar sin error.

2. **Cuando algo cambia, se relee todo**: la misma llamada que «actualizar».
   Nada nuevo que pueda desincronizarse: la idempotencia de `indexar` ya está
   probada, y el vigía sólo decide **cuándo**.

3. **Tres segundos de calma** (`PAUSA`). Cada evento que importa reinicia la
   cuenta; la pasada sale cuando llevan tres segundos sin llegar. Copiar
   cincuenta archivos de golpe es una o dos pasadas, no cincuenta.

4. **Nunca dos a la vez.** Si llega un cambio con una pasada en marcha, se
   anota y, al terminar ésa, sale **una** más. Sin eso, un archivo guardado a
   media pasada —después de que el escáner pasara por su carpeta— se quedaría
   fuera hasta el siguiente cambio.

5. **Lo que no importa no despierta a nadie.** Se ignoran:
   - las **lecturas** (`Access`) y los cambios de hora de acceso: el propio
     indexado lee los archivos, y sin esto se despertaría a sí mismo;
   - los **temporales**: `~$…` (el candado de Word y Excel), `.tmp`,
     `.crdownload`, `.part`, y el ruido que el escáner ya ignora
     (`desktop.ini`, `Thumbs.db`…);
   - lo que cuelga de carpetas que el escáner poda (`node_modules`, `.git`…).

   Un evento sin ruta, o un error del vigía (por ejemplo, la cola del sistema
   desbordada), **sí** cuenta: ante la duda se relee, que es barato y nunca
   pierde nada.

6. **obrain nunca escribe en tus carpetas**, así que indexar no puede
   provocar otra pasada: la base vive en `%LOCALAPPDATA%\obrain`.

7. **En la interfaz**: la pasada del vigía avisa al empezar (evento
   `indexando`) y el indicador de abajo a la izquierda pasa a «Leyendo tu
   bóveda…»; al terminar, a «Al día», y lo nuevo aparece en Recientes y en
   «Caduca pronto». **Sin aviso**: el «Hay 1 documento nuevo» que ya salía al
   terminar un indexado se reserva para «actualizar» y el arranque, porque con
   el vigía saldría cada vez que guardas algo (EXPERIENCE §7: el silencio es la
   característica).

## Alternativas consideradas

### A. Leer sólo lo que cambió

Más rápido con bóvedas grandes. Rechazada por el usuario: exige un pipeline
por archivo con sus casos propios (mover, renombrar, borrar, una carpeta
entera que llega de golpe), y cada uno es una forma nueva de que la base
discrepe del disco. Si algún día hace falta, la pieza cara es el hash (ver
«En contra») y se puede cachear por tamaño y fecha sin cambiar esto.

### B. Seguir vigilando con la ventana cerrada (bandeja del sistema)

Rechazada por el usuario: un proceso siempre en marcha y arrancar con Windows
para algo que se resuelve igual al abrir la app.

### C. El USN Change Journal

Es lo que hace instantáneo a Everything, pero es para volúmenes enteros,
necesita permisos y sólo existe en NTFS. Para unas pocas carpetas elegidas,
`ReadDirectoryChangesW` es la herramienta. Sigue congelado.

## Consecuencias

**A favor**

- Guardas un documento y aparece solo, con su texto, su sensibilidad y su
  fecha de caducidad, en unos cinco segundos.
- No hay una segunda forma de indexar: el vigía reutiliza «actualizar».

**En contra**

- **Cada pasada vuelve a calcular el hash de toda la bóveda.** Medido en el
  banco de pruebas (4 núcleos, disco en caché, que es lo normal cuando las
  pasadas se repiten): una pasada sin cambios cuesta **0,01 s** con la bóveda
  de prueba (11 documentos) y **0,34–0,39 s** con una de **1,5 GB** (300
  archivos de 5 MB). En frío —la primera tras encender— es lo que tarde el
  disco en leer esos GB. Con decenas de GB de fotos, el disco trabajará cada
  vez que algo cambie. Es lo que se eligió.
- Mientras dura una pasada, la búsqueda por significado cede el modelo al
  indexador y responde por palabras, como ya pasaba con «actualizar».
- `ReadDirectoryChangesW` sobre una carpeta de OneDrive también ve lo que
  hace OneDrive (sincronizar, liberar espacio). Eso lanza pasadas que no
  cambian nada; M1 no hidrata nunca un archivo, así que no descargan nada.
- **El código de Windows del vigía no se ha probado en Windows**: `notify` es
  multiplataforma y aquí se prueba con `inotify`. Falta verlo en la laptop del
  usuario.
