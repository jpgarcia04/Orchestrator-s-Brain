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
| **La interfaz nueva** (Svelte 5): escritorio con barra lateral, Inicio, carpetas en cuadrícula o lista, vista previa, Sensibles, buscador `Ctrl K`, el Mapa enlazado, noche y día | ✅ Probado en tu Windows (2026-09-23) |
| «Añadir carpeta» desde la app, o `obrain indexar <carpeta>` desde la terminal | |
| **Sensibilidad por carpeta**: marcas `01 Identidad` y todo lo de dentro es sensible, también lo que llegue después (la foto de la TIE incluida). Si la renombras o la mueves dentro del mismo disco, la marca va con ella | ✅ Probado en tu Windows (2026-09-23) |
| **Base cifrada** (SQLCipher) con la llave en Windows y una clave de recuperación; **PIN** opcional para ver lo sensible | ✅ Probado en tu Windows (2026-09-23). Falta el PIN, si lo quieres |
| **Leer fotos y PDF escaneados (OCR)** con el reconocedor que ya trae Windows, sin conexión: la foto de la TIE se encuentra buscando «extranjero» y sale sensible por su NIE | ✅ Probado en tu Windows (2026-09-23) |
| **Vencimientos**: lee cuándo caduca cada documento («VALIDEZ 15 11 2026» en la foto de la TIE, «vence el 30 de septiembre» en el contrato) y el Inicio te enseña **«Caduca pronto»**: lo que vence en 90 días y lo que ya venció. Cada fecha dice por qué la leyó y se corrige en un clic. **Aunque el OCR se coma la palabra** (tu pasaporte): la saca de la zona MRZ (`P<MEX<<…`) con su dígito de control, de la expedición más N años justos en un documento de identidad, o del año solo de la INE (ADR-0028) | Tu pasaporte no daba fecha (2026-09-23): el OCR no leyó «caducidad». Arreglado y comprobado con su texto; falta verlo en tu app |
| **Una carpeta fija para todo**: la base, su llave y el modelo viven en `C:\Users\pacox\.obrain`, abras obrain desde donde lo abras (una PowerShell, la terminal de Claude o la de Codex). La primera vez, obrain muda sola la base de `%LOCALAPPDATA%\obrain` (ADR-0029) | Tu modelo ya está ahí (2026-09-23). Falta abrir la app para que mude tu base |
| **Un documento dentro de otro**: el PDF que armaste para un trámite juntando otros (tus estados de cuenta, una constancia) dice **«Contiene»** y qué lleva, con su «¿por qué?» marcado en el texto; cada documento suelto dice **«Está dentro de»** y te lleva al paquete por donde está. El paquete **es tan sensible como lo que lleva** (el pasaporte de `01 Identidad` dentro del PDF de la visa lo tapa), y en la búsqueda lo de dentro sale justo debajo de su paquete (ADR-0030) | Probado con tu base de agosto: 14 paquetes, ninguno falso. Falta verlo en tu app |
| La app se pone al día sola al abrirse, y tiene botón «actualizar» | |
| **Vigila tus carpetas en vivo** mientras está abierta: guardas un documento y aparece solo, a los 3 segundos de dejar de tocar la carpeta, sin avisos (sólo el indicador de abajo a la izquierda) | ✅ Probado en tu Windows (2026-09-23) |
| **Papelera**: lo que borras sale de Recientes, «Caduca pronto», el explorador, el mapa y la búsqueda, y se queda 30 días en la vista «Papelera»; después se olvida lo que se leyó de él (o antes, con «Olvidar ya» o «Vaciar papelera»). Mover y renombrar ya no dejan documentos ni carpetas repetidos | ✅ Probado en tu Windows (2026-09-23) |

Núcleo en Rust con 452 tests, **todos en verde en tu Windows** (2026-09-23;
hasta ese día sólo se habían corrido en Linux), interfaz en Tauri + Svelte 5,
con el mapa en PixiJS.

## Cómo usarlo hoy (Windows)

Hace falta [Rust](https://rustup.rs), [Node.js](https://nodejs.org) y, **desde la
Fase 3, Perl** —la base se cifra con SQLCipher y OpenSSL se compila dentro del
programa, y eso pide Perl una vez—. En PowerShell:

```powershell
winget install StrawberryPerl.StrawberryPerl   # una vez; luego cierra y abre la terminal
```

(Ya instalado en tu laptop el 2026-09-23.) **Y un detalle por el apóstrofo** de
`Orchestrator's-Brain`: OpenSSL escribe la ruta de compilación dentro de un
script de Perl y el `'` lo rompe (`Unknown regexp modifier` en
`configdata.pm`). Mientras la carpeta se llame así, compila fuera de ella:

```powershell
$env:CARGO_TARGET_DIR = "$env:USERPROFILE\obrain-target"   # en cada terminal nueva
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

# (sólo si no lo tienes; el tuyo ya está en C:\Users\pacox\.obrain\models)
# bajar el modelo de búsqueda por significado, ~465 MB
cargo run --release --bin obrain-embed -- "$env:USERPROFILE\.obrain\brain.obrain" --descargar-modelo
```

Sin ese modelo la búsqueda funciona igual por palabras; el buscador te dice
abajo a la derecha en qué modo está.

**Todo lo de obrain vive en una carpeta fija: `C:\Users\pacox\.obrain`** —la
base (`brain.obrain`), su llave y el modelo—, y la comparten la orden `obrain`
y la aplicación, las abras desde donde las abras (ADR-0029). Hasta el
2026-09-23 vivía en `%LOCALAPPDATA%\obrain`: **la primera vez que abras esta
versión, obrain la muda sola** (si otra obrain la tiene abierta, esa vez la
usa donde está y la muda la siguiente). Por qué: Windows le da a la app de
Claude y a la de Codex su propia copia de `AppData`, y lo que se abría desde
ellas veía otra base; eso fue el «cifrado a medias». Otras órdenes útiles:

```powershell
cargo run --release --bin obrain -- bibliotecas   # qué carpetas tienes registradas
cargo run --release --bin obrain -- indexar       # volver a leer todas
cargo run --release --bin obrain -- vencimientos  # lo que caduca en 90 días o ya caducó
cargo run --release --bin obrain -- paquetes      # los documentos que llevan otros dentro
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
  Cada apartado de la vista previa —el documento, «Lo que sé de este
  documento», «Relacionado», «Detalles técnicos»— **se pliega pulsando su
  título**, y se queda como lo dejaste para los demás documentos y al volver a
  abrir la app. «Descargado de» dice sólo el sitio (`chatgpt.com`); la
  dirección entera, al pasar el ratón o en «Detalles técnicos».
- **Un documento dentro de otro:** en la vista previa de un PDF que junta
  otros —el que armaste para la visa con tus estados de cuenta— sale el
  apartado **«Contiene»** con cada uno; su **«¿por qué?»** dice *«Todo el
  texto de «Movimientos marzo» está en este documento: sus 9 párrafos,
  seguidos y en el mismo orden»* y lo marca arriba. En el documento suelto,
  «Relacionado» dice **«Está dentro de»** y un clic abre el paquete por donde
  está. Si lo de dentro es sensible (tu pasaporte en `01 Identidad`), el
  paquete también, y en «Lo que sé» dice por qué: *«porque contiene
  «Pasaporte», que está en tu carpeta «01 Identidad»»*. En la búsqueda, lo de
  dentro sale justo debajo de su paquete, un poco metido. Sólo ve lo que se
  leyó igual: un PDF que junta PDF con texto sí; una foto dentro de un
  escaneo, no (el OCR nunca lee dos veces igual).
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
  Si renombras o mueves una carpeta marcada **dentro del mismo disco**, la
  marca va con ella: obrain reconoce la carpeta por su identidad en NTFS (su
  `FileID`), no por su nombre. La marca es de la carpeta, no del nombre: si
  creas otra con el nombre viejo, ésa nace sin marca. Si la mueves **a otro
  disco** (un USB), la marca se queda en la ruta vieja; «Sensibles» te lo
  avisa en rojo y basta con volver a marcarla. Lo mismo con una carpeta que
  renombraste antes de esta versión.
- **Seguridad:** el escudo de abajo a la izquierda. Ahí ves que la base está
  cifrada, vuelves a ver tu clave de recuperación y pones un **PIN**: con PIN,
  lo sensible no se enseña —ni en la vista previa, ni en la búsqueda, ni al
  abrirlo— hasta escribirlo, y se queda a la vista 5 minutos desde la última vez
  que lo miras. ¿Olvidaste el PIN? Se restablece con la clave de recuperación.
- **Caduca pronto:** en el Inicio, entre el buscador y «Recientes», sólo si hay
  algo: lo que caduca en los próximos 90 días y lo que ya caducó (en rojo), con
  cuántos días faltan. La fecha sale del propio documento —también de las
  fotos, con el OCR—: de la palabra («caduca», «validez», «vigencia»…), de
  la **zona MRZ** de un pasaporte o del dorso de una tarjeta (la que más
  manda: lleva dígito de control; eso sí, el OCR de Windows casi nunca la
  lee en una foto, así que cuenta sobre todo con los PDF de una app de
  escaneo, que ya traen su texto), del **año solo** de la INE («VIGENCIA
  2023 - 2033» → 31/12/2033) o, si el OCR se comió la palabra, de **dos
  fechas a N años justos** en un documento de identidad (la expedición y la
  caducidad de tu pasaporte). En la vista previa, «Caduca el» tiene su
  **«¿por qué?»** (cómo la leyó, y el tramo marcado en el texto), **«es correcto»**
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
| **3** | **Documentos personales**: ✅ sensibilidad por carpeta (y la marca sigue a la carpeta si la renombras) · ✅ cifrado y PIN · ✅ OCR · ✅ vencimientos · ✅ vigilar la carpeta en vivo · ✅ papelera | ✅ Hecha y probada en tu Windows (2026-09-23); los vencimientos, afinados con tu pasaporte (ADR-0028), falta verlos en tu app |
| 3+ | **Tu bóveda, relacionada**: ✅ un documento dentro de otro (ADR-0030) · tus colecciones (ADR-0031) | En curso: lo de dentro, hecho (2026-09-23); falta verlo en tu app. Las colecciones, lo siguiente |
| 4 | La visión original: trámites reconstruidos, la pregunta de la visa | Congelada (JP, 2026-09-23: primero usar la app en Windows) |

**Decidido el 2026-09-23, por construir** (el orden, después de probar la app
en tu Windows): ~~**un documento dentro de otro**~~ **hecho el mismo día
(ADR-0030)**: obrain dice que el PDF de la visa *contiene* los estados de
cuenta que también tienes sueltos, sin IA. Antes de programar se midió con
tu base de agosto y se cambió una cosa, que elegiste tú: **por párrafos, no
por fragmentos** (por fragmentos salía 1 de los 14 paquetes; por párrafos,
los 14). También elegiste que **el paquete herede la sensibilidad** de lo que
lleva y que en la búsqueda **lo de dentro vaya debajo** de su paquete.
Sigue **tus colecciones** —agrupar a mano, un documento en varias, vistas
como una lente sobre el mapa— (ADR-0031). Y **no** se indexa la Papelera de
Windows.

**Congelado** (no se borra, no se sigue): sesiones de trabajo y puentes,
detección de versiones, el USN Change Journal, sincronización entre
dispositivos.

~~**Por confirmar (JP):**~~ **Confirmado (2026-09-22).** Revisaste las capturas
de la Fase 2 y la interfaz se queda como está: el explorador es la pantalla de
inicio, la galaxia es la vista «Mapa» y el aspecto de noche es el de por
defecto (el de día, a un clic). Queda escrito en
[ADR-0019](adr/0019-el-explorador-es-el-escritorio.md), que sustituye al 0013.
**Y ya corre en tu Windows (2026-09-23):** la abriste con tu carpeta de
pruebas y funcionaron el cifrado, Sensibles con su «¿por qué?», el OCR, el
vigía, la papelera y la carpeta marcada.

**Antes de meter tu pasaporte de verdad**, tres cosas:

1. ~~Corre la app y mira en «Seguridad» que la base está cifrada.~~ Hecho el
   2026-09-23.
2. ~~Guarda la clave de recuperación.~~ Hecho (la de la base nueva; la
   primera que guardaste ese día ya no sirve).
3. **Activa el Cifrado de dispositivo** de Windows si tu laptop lo tiene
   (Configuración › Privacidad y seguridad): cifrar la base de obrain **no
   cifra tus archivos originales**, y lo que hubo en claro puede seguir en el
   espacio libre del SSD hasta que se sobrescriba.

Y para ver el OCR: dale a «actualizar» con alguna foto de un documento en tu
biblioteca y búscala por algo que diga. En la vista previa, debajo de la foto,
sale «Texto reconocido en la imagen» y, en el «¿por qué?», *«el OCR de Windows
(es-ES)»*. Si no sale nada, Windows no tiene el paquete de reconocimiento de tu
idioma: Configuración › Hora e idioma › Idioma › Español › Opciones.

Y para ver la carpeta fija (ADR-0029): con la app **cerrada**, trae la versión
nueva (`git pull`) y ábrela **desde una PowerShell normal** como siempre. En la
terminal tiene que salir `▸ obrain vive ahora en C:\Users\pacox\.obrain`, y
abajo a la izquierda de la app, la ruta nueva. Tus documentos, tu clave de
recuperación y tus marcas siguen igual (es la misma base, cambiada de sitio).
Y en el buscador (`Ctrl K`), abajo a la derecha, tiene que decir que busca
**por significado**: el modelo ya está en `C:\Users\pacox\.obrain\models`.

Y para ver los vencimientos: abre la app con esta versión (se pone al día
sola al abrirse) y pulsa tu pasaporte. En la vista previa tiene que salir
**«Caduca el …»** con la fecha de tu pasaporte, y en su «¿por qué?»: *«No
leí la palabra «caducidad» —el OCR no la vio—, pero es un documento de
identidad y dos de sus fechas están a 10 años justos…»*, con las dos fechas
marcadas en el texto. Si es la correcta, «es correcto». Lo mismo con
cualquier documento que diga cuándo caduca —«VALIDEZ», «válida hasta»,
«vence el», «caduca», la INE con su «VIGENCIA»—: si es dentro de 90 días o
ya pasó, aparece en «Caduca pronto» del Inicio. Si el OCR leyó mal un
número, «cambiar» lo arregla.

Y para ver un documento dentro de otro (ADR-0030): si en tu bóveda hay algún
PDF que armaste juntando otros —el de la visa con tus estados de cuenta, un
expediente—, ábrelo en la vista previa. Tiene que salir el apartado
**«Contiene»** con los que lleva; su «¿por qué?» los marca en el texto.
Desde uno suelto, «Está dentro de» te lleva al paquete por donde está. Desde
la terminal, `obrain paquetes` los lista todos. Si no sale nada que debería
salir, dime cuál (su nombre basta): puede que el paquete retoque lo que junta
o que sean escaneos, y eso lo miramos enmascarado como el pasaporte. Y si
alguno de lo de dentro está en una carpeta marcada, el paquete tiene que salir
tapado, con *«porque contiene …»* en «Lo que sé».

Y para ver la papelera: con la app abierta, **borra** un documento de prueba
de tu bóveda (a la Papelera de Windows, como siempre). A los pocos segundos
desaparece de Recientes y de su carpeta y aparece en «Papelera» con «se olvida
en 30 días». Luego **restáuralo** desde la Papelera de Windows: tiene que
volver solo a su carpeta, con su fecha de caducidad y lo que le hubieras
corregido. Si en vez de eso aparece como documento nuevo, avísame: significa
que Windows le cambió la identidad (el `FileID`) al restaurarlo.

Y para ver que la marca sigue a la carpeta: marca una carpeta de prueba como
sensible, dale a «actualizar» una vez (así obrain apunta su identidad) y
**renómbrala** en el Explorador de Windows. A los pocos segundos tiene que
salir con su nombre nuevo, con el candado, y en «Sensibles» con sus documentos
(no en rojo). Muévela a otra carpeta de la bóveda y lo mismo.

## Cómo seguir en otra sesión

El trabajo vive en GitHub, en la rama `claude/orchestrators-brain-fase-3-wdiheq`
(todavía no está en `main`). En tu laptop, desde la carpeta del repositorio:

```powershell
git fetch origin
git checkout claude/orchestrators-brain-fase-3-wdiheq   # la primera vez
git pull                                                # las siguientes
```

Para empezar una conversación nueva, pega
[`archivo/prompts/despues-de-la-carpeta-fija.md`](archivo/prompts/despues-de-la-carpeta-fija.md).

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
