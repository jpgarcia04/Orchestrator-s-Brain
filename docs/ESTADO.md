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
| Marcar lo sensible y no dejarlo salir de la máquina | La interfaz nueva (Fase 2) |
| **Bibliotecas**: el explorador empieza en tu carpeta, no en la raíz del disco | Vencimientos (*¿qué me caduca pronto?*) |
| Carpetas con su nombre, fechas y tamaños legibles | |
| `obrain indexar <carpeta>`: todo el proceso en una orden | |
| La app se pone al día sola al abrirse, y tiene botón «actualizar» | |

Núcleo en Rust con 274 tests (uno de ellos sólo corre en Windows), interfaz en
Tauri + TypeScript + PixiJS.

## Cómo usarlo hoy (Windows)

Hace falta [Rust](https://rustup.rs) y [Node.js](https://nodejs.org). Desde la
carpeta del repositorio, en PowerShell:

```powershell
# 1 · Indexar tu bóveda (la primera vez compila; tarda unos minutos)
cd core
cargo run --release --bin obrain -- indexar "C:\Users\pacox\Segundo Cerebro"

# 2 · (una sola vez) bajar el modelo de búsqueda por significado, ~465 MB
cargo run --release --bin obrain-embed -- "$env:LOCALAPPDATA\obrain\brain.obrain" --descargar-modelo

# 3 · Abrir la aplicación
cd ..\ui
npm install
npm run app
```

La base vive en `%LOCALAPPDATA%\obrain\brain.obrain` y la comparten la orden
`obrain` y la aplicación. **Si esa base ya tiene lo que escaneaste en agosto**
(tu perfil entero), empieza de cero para la bóveda: cierra la app y renombra
`brain.obrain` antes del primer `obrain indexar`. Otras órdenes útiles:

```powershell
cargo run --release --bin obrain -- bibliotecas   # qué carpetas tienes registradas
cargo run --release --bin obrain -- indexar       # volver a leer todas
```

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
| **2** | **La interfaz nueva en Svelte 5**: el explorador como escritorio estilo sistema operativo, la galaxia como vista «Mapa» enlazada, `Ctrl+K`, vista previa rápida. Primero, una maqueta para elegir el aspecto | ⏭ Siguiente |
| 3 | Documentos personales: OCR, vencimientos, sensibilidad por carpeta, cifrado y PIN | Pendiente |
| 4 | La visión original: trámites reconstruidos, la pregunta de la visa | Congelada |

**Congelado** (no se borra, no se sigue): sesiones de trabajo y puentes,
detección de versiones, el USN Change Journal, sincronización entre
dispositivos.

**Decisión abierta antes de la Fase 2:** si la pantalla principal es el
explorador (propuesta) o la galaxia. Ver §10 del maestro.

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
ui/              la aplicación de escritorio (Tauri)
docs/            esto, el maestro, la experiencia, los ADR y el archivo
prototypes/      pruebas de concepto del render y de la dirección visual
```
