# Estado del proyecto — empieza aquí

**Última revisión:** 2026-09-23

Una página para retomar el proyecto sin leer las 2.000 líneas de agosto. El
diseño completo está en [`PROJECT-MASTER.md`](PROJECT-MASTER.md); el porqué de
cada decisión, en [`adr/`](adr/); la historia de cada hito, en la
[bitácora](archivo/bitacora-agosto-2026.md).

## Qué es, en una frase

Un **segundo cerebro para tus documentos personales**: dejas en una carpeta tu
pasaporte, tus pólizas, tu matrícula o tus contratos, y el programa los lee, los
relaciona y te deja encontrarlos por lo que significan, sin que nada salga de tu
computadora.

## Dónde estamos

| Funciona | Falta |
|---|---|
| Escanear una carpeta y reconocer cada archivo aunque lo muevas o renombres | |
| Leer el texto de PDF, Word, HTML y texto plano | |
| Buscar por palabras y por significado (con el modelo local descargado) | |
| Marcar lo sensible y no dejarlo salir de la máquina | |
| **Bibliotecas**: el explorador empieza en tu carpeta, no en la raíz del disco | |
| Carpetas y documentos con nombres legibles, fechas y tamaños claros | |
| **La interfaz nueva** (Svelte 5): escritorio con barra lateral, Inicio, carpetas en cuadrícula o lista, vista previa, Sensibles, buscador `Ctrl K`, el Mapa enlazado, noche y día | |
| «Añadir carpeta» desde la app, o `obrain indexar <carpeta>` desde la terminal | |
| **Sensibilidad por carpeta**: marcas `01 Identidad` y todo lo de dentro es sensible, también lo que llegue después (la foto de la TIE incluida) | |
| **Base cifrada** (SQLCipher) con la llave en Windows y una clave de recuperación; **PIN** opcional para ver lo sensible | Probarlo en tu Windows: DPAPI sólo se ha comprobado que compila |
| **Leer fotos y PDF escaneados (OCR)** con el reconocedor que ya trae Windows, sin conexión: la foto de la TIE se encuentra buscando «extranjero» y sale sensible por su NIE | Probarlo en tu Windows: el OCR sólo se ha comprobado que compila |
| **Vencimientos**: lee cuándo caduca cada documento («VALIDEZ 15 11 2026» en la foto de la TIE, «vence el 30 de septiembre» en el contrato) y el Inicio te enseña **«Caduca pronto»**: lo que vence en 90 días y lo que ya venció. Cada fecha dice por qué la leyó y se corrige en un clic | Probarlo en tu Windows, con el OCR de verdad |
| La app se pone al día sola al abrirse, y tiene botón «actualizar» | |
| **Vigila tus carpetas en vivo** mientras está abierta: guardas un documento y aparece solo, a los 3 segundos de dejar de tocar la carpeta, sin avisos (sólo el indicador de abajo a la izquierda) | Probarlo en tu Windows: aquí se ha probado con el vigía de Linux |
| **Papelera**: lo que borras sale de Recientes, «Caduca pronto», el explorador, el mapa y la búsqueda, y se queda 30 días en la vista «Papelera»; después se olvida lo que se leyó de él (o antes, con «Olvidar ya» o «Vaciar papelera»). Mover y renombrar ya no dejan documentos ni carpetas repetidos | Probarlo en tu Windows: sacar algo de la Papelera de Windows y ver que vuelve solo |

Núcleo en Rust con 383 tests (uno de ellos sólo corre en Windows), interfaz en
Tauri + Svelte 5, con el mapa en PixiJS.

## Cómo usarlo hoy (Windows)

Hace falta [Rust](https://rustup.rs), [Node.js](https://nodejs.org) y, **desde la
Fase 3, Perl** —la base se cifra con SQLCipher y OpenSSL se compila dentro del
programa, y eso pide Perl una vez—. En PowerShell:

```powershell
winget install StrawberryPerl.StrawberryPerl   # una vez; luego cierra y abre la terminal
```

Desde la carpeta del repositorio:

```powershell
# 1 · Abrir la aplicación (la primera vez compila; tarda unos minutos)
cd ui
npm install
npm run app
```

**La primera vez que abras esta versión, la base se cifra** (también la de
agosto, si la tienes) y la app te enseña tu **clave de recuperación**: apúntala
en tu gestor de contraseñas o en un papel. Te la seguirá enseñando al abrir hasta
que marques «Ya la guardé». Con ella se abre la base si Windows pierde su llave
(otro equipo, Windows reinstalado).

Con la base vacía, el Inicio te ofrece **«Elegir carpeta»**: eliges tu bóveda y
la app la lee en segundo plano (verás «Leyendo tu bóveda…» abajo a la
izquierda). Más adelante, **«Añadir carpeta»** en la barra lateral hace lo mismo
con otra. Lo mismo desde la terminal, si lo prefieres:

```powershell
cd core
cargo run --release --bin obrain -- indexar "C:\Users\pacox\Segundo Cerebro"

# (una sola vez) bajar el modelo de búsqueda por significado, ~465 MB
cargo run --release --bin obrain-embed -- "$env:LOCALAPPDATA\obrain\brain.obrain" --descargar-modelo
```

Sin ese modelo la búsqueda funciona igual por palabras; el buscador te dice
abajo a la derecha en qué modo está.

La base vive en `%LOCALAPPDATA%\obrain\brain.obrain` y la comparten la orden
`obrain` y la aplicación. **Si esa base ya tiene lo que escaneaste en agosto**
(tu perfil entero), empieza de cero para la bóveda: cierra la app y renombra
`brain.obrain` antes del primer `obrain indexar`. Otras órdenes útiles:

```powershell
cargo run --release --bin obrain -- bibliotecas   # qué carpetas tienes registradas
cargo run --release --bin obrain -- indexar       # volver a leer todas
cargo run --release --bin obrain -- vencimientos  # lo que caduca en 90 días o ya caducó
cargo run --release --bin obrain -- papelera      # lo que ya no está en tus carpetas (30 días)
cargo run --release --bin obrain -- clave         # tu clave de recuperación
cargo run --release --bin obrain -- recuperar XXXXX-XXXXX-XXXXX-XXXXX-XXXXX   # en otro equipo
```

Junto a la base vive `brain.obrain.llave`: **van juntas**. Una copia de
seguridad de la base sin su clave de recuperación no se abre en otro equipo.

### La aplicación, en dos minutos

- **Buscar:** `Ctrl K`, o simplemente empieza a escribir en cualquier sitio.
  Cada resultado dice si coincidió *por palabras* o *por significado*.
- **Un clic** en un documento lo enseña en la vista previa; **doble clic** lo
  abre con su programa. Lo sensible sale borroso hasta que pulsas «Mostrar».
- **Mapa:** cada estrella es una carpeta; pulsarla la abre en el explorador.
  Desde un documento, «Ver en el mapa» enseña con qué se relaciona.
- **`Esc`** deshace un escalón: cierra el buscador, la vista previa o la lente
  del mapa.
- **«¿Por qué?» de la sensibilidad:** en la vista previa, junto a «Sensible»,
  dice con palabras si lo leyó en el texto (*«hay un DNI o NIE, con su letra de
  control correcta»*, y el NIE sale marcado en el texto) o si fue por su
  nombre o su carpeta. El NIE no se guarda en ningún log: el tramo se vuelve a
  buscar al enseñarlo.
- **Carpeta sensible:** dentro de una carpeta, el escudo de arriba a la derecha
  («Marcar como sensible») hace sensible todo lo que tiene dentro, también lo
  que llegue después. Para quitar la marca, dos clics. La vista «Sensibles»
  enseña qué carpetas marcaste. Desde la terminal:
  `obrain sensible "C:\...\01 Identidad"` (y `--quitar` para deshacerlo).
  Ojo: si renombras o mueves una carpeta marcada, la marca se queda en la ruta
  vieja; «Sensibles» te lo avisa en rojo y basta con volver a marcarla.
- **Seguridad:** el escudo de abajo a la izquierda. Ahí ves que la base está
  cifrada, vuelves a ver tu clave de recuperación y pones un **PIN**: con PIN,
  lo sensible no se enseña —ni en la vista previa, ni en la búsqueda, ni al
  abrirlo— hasta escribirlo, y se queda a la vista 5 minutos desde la última vez
  que lo miras. ¿Olvidaste el PIN? Se restablece con la clave de recuperación.
- **Caduca pronto:** en el Inicio, entre el buscador y «Recientes», sólo si hay
  algo: lo que caduca en los próximos 90 días y lo que ya caducó (en rojo), con
  cuántos días faltan. La fecha sale del propio documento —también de las
  fotos, con el OCR—. En la vista previa, «Caduca el» tiene su **«¿por qué?»**
  (con qué palabra la leyó, y el tramo marcado en el texto), **«es correcto»**
  y **«cambiar»**: otra fecha o **«no caduca»**. Lo que digas tú gana siempre,
  y un documento que ya renovaste sale de la tarjeta diciéndole «no caduca».
  Si un documento no dice cuándo caduca (o el OCR no lo leyó), en su vista
  previa sale en gris **«Caduca el — añadir»**: se la pones tú y entra en la
  tarjeta como tuya.
  Con el PIN bloqueado, lo sensible sale sin fecha («fecha protegida con tu
  PIN»).
- **Papelera:** en la barra lateral, debajo de «Sensibles» (con su cuenta si
  hay algo). Lo que borras de tu bóveda aparece ahí con la carpeta donde
  estaba y cuántos días le quedan; un clic lo enseña en la vista previa con lo
  que se leyó de él. A los **30 días** obrain olvida su texto; «Olvidar ya» o
  «Vaciar papelera» (dos clics) lo adelantan. **No toca tus archivos**: el
  archivo ya lo borraste tú. Si lo sacas de la Papelera de Windows —o lo
  vuelves a copiar con el mismo contenido—, vuelve solo con todo lo que sabías
  (tus correcciones incluidas). Una carpeta de biblioteca que no está entera
  (un USB desconectado) no manda nada a la papelera. Desde la terminal:
  `obrain papelera` y `obrain papelera --vaciar`.
- **Al día solo:** con la app abierta, lo que guardes, cambies o borres en tu
  bóveda se lee solo tres segundos después de que dejes de tocarla. Abajo a la
  izquierda verás «Leyendo tu bóveda…» un momento y luego «Al día»; lo nuevo
  sale en Recientes y, si caduca pronto, en la tarjeta. Los temporales (`~$` de
  Word, descargas a medias) no cuentan. «Actualizar» sigue ahí por si acaso.
- El sol/luna de abajo a la izquierda cambia entre **noche** y **día**.

## Cómo organizar tu bóveda

El programa entiende mejor lo que tú ordenas bien. Una propuesta:

```
Segundo Cerebro/
  01 Identidad/    pasaporte, TIE, acta de nacimiento, CURP
  02 Salud/        pólizas, tarjeta sanitaria, informes, recetas
  03 Estudios/     matrícula, certificados, títulos, homologaciones
  04 Trámites/     extranjería, visados, citas
  05 Vivienda/     contrato, recibos
  06 Dinero/       banco, nóminas, facturas, impuestos
  07 Trabajo/      CV, contratos
  08 Viajes/
```

Y nombres que se lean solos, con la fecha delante:
`2026-09-01 Póliza seguro médico (Salud Mediterránea).pdf`.

## Qué sigue

| Fase | Qué | Estado |
|---|---|---|
| 0 | Limpieza: documentación a la medida, README honesto | ✅ Hecha |
| 1 | El núcleo para tu carpeta: bibliotecas, nombres, `obrain indexar`, «actualizar» | ✅ Hecha |
| 2 | La interfaz nueva en Svelte 5: el explorador como escritorio estilo sistema operativo, la galaxia como vista «Mapa» enlazada, `Ctrl+K`, vista previa rápida | ✅ Hecha |
| **3** | **Documentos personales**: ✅ sensibilidad por carpeta · ✅ cifrado y PIN · ✅ OCR · ✅ vencimientos · ✅ vigilar la carpeta en vivo · ✅ papelera | ✅ Hecha (falta probarla en tu Windows) |
| 4 | La visión original: trámites reconstruidos, la pregunta de la visa | Congelada |

**Congelado** (no se borra, no se sigue): sesiones de trabajo y puentes,
detección de versiones, el USN Change Journal, sincronización entre
dispositivos.

~~**Por confirmar (JP):**~~ **Confirmado (2026-09-22).** Revisaste las capturas
de la Fase 2 y la interfaz se queda como está: el explorador es la pantalla de
inicio, la galaxia es la vista «Mapa» y el aspecto de noche es el de por
defecto (el de día, a un clic). Queda escrito en
[ADR-0019](adr/0019-el-explorador-es-el-escritorio.md), que sustituye al 0013.
Falta probarla en tu Windows de verdad: hasta hoy sólo se ha visto en el
navegador del banco de pruebas (el 2026-09-22 aún no la habías corrido).

**Antes de meter tu pasaporte de verdad**, tres cosas:

1. **Corre la app una vez en tu Windows** y mira en «Seguridad» que diga *«Tu
   base está cifrada»* (con la llave de Windows). El código de DPAPI sólo se ha
   podido comprobar que compila; que funcione, se ve ahí.
2. **Guarda la clave de recuperación** que te enseña la app.
3. **Activa el Cifrado de dispositivo** de Windows si tu laptop lo tiene
   (Configuración › Privacidad y seguridad): cifrar la base de obrain **no
   cifra tus archivos originales**, y lo que hubo en claro puede seguir en el
   espacio libre del SSD hasta que se sobrescriba.

Y para ver el OCR: dale a «actualizar» con alguna foto de un documento en tu
biblioteca y búscala por algo que diga. En la vista previa, debajo de la foto,
sale «Texto reconocido en la imagen» y, en el «¿por qué?», *«el OCR de Windows
(es-ES)»*. Si no sale nada, Windows no tiene el paquete de reconocimiento de tu
idioma: Configuración › Hora e idioma › Idioma › Español › Opciones.

Y para ver los vencimientos: si esa foto (o cualquier documento) dice cuándo
caduca —«VALIDEZ», «válida hasta», «vence el», «caduca»—, en la vista previa
sale «Caduca el» y, si es dentro de 90 días o ya pasó, aparece en «Caduca
pronto» del Inicio. Si el OCR leyó mal un número, «cambiar» lo arregla.

## Mapa del repositorio

```
core/            el núcleo en Rust
  obrain-keys/     la llave de la base: DPAPI y la clave de recuperación
  obrain-core/     el log de aserciones (la verdad), cifrado con SQLCipher
  obrain-scan/     recorrer el disco, identidad de archivos, bibliotecas
  obrain-link/     relaciones sin IA (carpetas, duplicados, descargas)
  obrain-ocr/      leer fotos y escaneos con el OCR de Windows
  obrain-extract/  leer texto y el índice de búsqueda por palabras
  obrain-guard/    sensibilidad y lo que nunca sale de la máquina
  obrain-expiry/   vencimientos: la fecha de caducidad, leída del texto
  obrain-embed/    búsqueda por significado con un modelo local
  obrain-api/      lo que ve la interfaz, y la orden `obrain`
ui/              la aplicación de escritorio (Tauri + Svelte 5)
  src/vistas/      Inicio, Carpeta, Mapa, Sensibles
  src/componentes/ barra lateral, cabecera, buscador, vista previa…
  src/lib/         el estado de la app, los nombres legibles, el mapa
  src-tauri/       el puente entre la interfaz y el núcleo
docs/            esto, el maestro, la experiencia, los ADR y el archivo
prototypes/      pruebas de concepto del render y de la dirección visual
```
