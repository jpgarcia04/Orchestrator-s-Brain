# Estado del proyecto — empieza aquí

**Última revisión:** 2026-09-22

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
| Escanear una carpeta y reconocer cada archivo aunque lo muevas o renombres | **Leer fotos y PDF escaneados (OCR)**: hoy una foto de tu pasaporte no tiene texto que buscar |
| Leer el texto de PDF, Word, HTML y texto plano | **Cifrado de la base**: hoy es un SQLite sin cifrar |
| Buscar por palabras y por significado (con el modelo local descargado) | Vigilar la carpeta en vivo: hoy se pone al día al abrir la app o con «actualizar» |
| Marcar lo sensible y no dejarlo salir de la máquina | Vencimientos (*¿qué me caduca pronto?*) |
| **Bibliotecas**: el explorador empieza en tu carpeta, no en la raíz del disco | |
| Carpetas y documentos con nombres legibles, fechas y tamaños claros | |
| **La interfaz nueva** (Svelte 5): escritorio con barra lateral, Inicio, carpetas en cuadrícula o lista, vista previa, Sensibles, buscador `Ctrl K`, el Mapa enlazado, noche y día | |
| «Añadir carpeta» desde la app, o `obrain indexar <carpeta>` desde la terminal | |
| **Sensibilidad por carpeta**: marcas `01 Identidad` y todo lo de dentro es sensible, también lo que llegue después (la foto de la TIE incluida) | |
| La app se pone al día sola al abrirse, y tiene botón «actualizar» | |

Núcleo en Rust con 302 tests (uno de ellos sólo corre en Windows), interfaz en
Tauri + Svelte 5, con el mapa en PixiJS.

## Cómo usarlo hoy (Windows)

Hace falta [Rust](https://rustup.rs) y [Node.js](https://nodejs.org). Desde la
carpeta del repositorio, en PowerShell:

```powershell
# 1 · Abrir la aplicación (la primera vez compila; tarda unos minutos)
cd ui
npm install
npm run app
```

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
```

### La aplicación, en dos minutos

- **Buscar:** `Ctrl K`, o simplemente empieza a escribir en cualquier sitio.
  Cada resultado dice si coincidió *por palabras* o *por significado*.
- **Un clic** en un documento lo enseña en la vista previa; **doble clic** lo
  abre con su programa. Lo sensible sale borroso hasta que pulsas «Mostrar».
- **Mapa:** cada estrella es una carpeta; pulsarla la abre en el explorador.
  Desde un documento, «Ver en el mapa» enseña con qué se relaciona.
- **`Esc`** deshace un escalón: cierra el buscador, la vista previa o la lente
  del mapa.
- **Carpeta sensible:** dentro de una carpeta, el escudo de arriba a la derecha
  («Marcar como sensible») hace sensible todo lo que tiene dentro, también lo
  que llegue después. Para quitar la marca, dos clics. La vista «Sensibles»
  enseña qué carpetas marcaste. Desde la terminal:
  `obrain sensible "C:\...\01 Identidad"` (y `--quitar` para deshacerlo).
  Ojo: si renombras o mueves una carpeta marcada, la marca se queda en la ruta
  vieja; «Sensibles» te lo avisa en rojo y basta con volver a marcarla.
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
| **3** | **Documentos personales**: ✅ sensibilidad por carpeta · cifrado y PIN (a decidir contigo) · OCR · vencimientos | ◐ En curso |
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

**Antes de meter tu pasaporte de verdad:** la base todavía no está cifrada. Marcar
una carpeta como sensible la tapa en pantalla y la deja fuera de cualquier modelo
remoto, pero no cifra nada en el disco. El cifrado es la siguiente pieza.

## Mapa del repositorio

```
core/            el núcleo en Rust
  obrain-core/     el log de aserciones (la verdad)
  obrain-scan/     recorrer el disco, identidad de archivos, bibliotecas
  obrain-link/     relaciones sin IA (carpetas, duplicados, descargas)
  obrain-extract/  leer texto y el índice de búsqueda por palabras
  obrain-guard/    sensibilidad y lo que nunca sale de la máquina
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
