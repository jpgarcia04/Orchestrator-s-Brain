# ADR-0042 — `--abrir` solo abre documentos

- **Estado:** Propuesta (2026-11-26, desde el proyecto Ecosistema). Pendiente de que JP la acepte al
  revisar el PR que la construye.
- **Fecha:** 2026-11-26
- **Amplía:** [ADR-0041](0041-abrir-desde-aetheris-sin-ventana.md). `--revelar`, `--mostrar`, la
  puerta MCP y `SoloNormal` no cambian.
- **Toca:** `core/obrain-api/src/desde_aetheris.rs`, `ui/src-tauri/src/main.rs` (solo pasa la
  acción) y [PUERTA-AETHERIS](../PUERTA-AETHERIS.md).

## Contexto

Con la 0041, `obrain-app --abrir <id>` abre con el programa que Windows tenga asociado cualquier
archivo que la puerta podría servir. La puerta decide si un archivo es **normal** (no sensible, ni de
bóveda, ni copia de algo protegido), no si es un **documento**. En una carpeta indexada puede haber
un `.exe`, un `.lnk`, un `.bat`, un `.ps1`, un `.url` o un `.msi`, y «abrirlo» es ejecutarlo. Una
página `.html` o un `.svg` llevan código al navegador. El botón «Abrir» de Aetheris no debe ser un
botón de ejecutar.

## Opciones

1. **Nada.** Confiar en que las carpetas de JP no tengan ejecutables. No se puede prometer.
2. **Lista de prohibidas.** Siempre se queda corta: Windows tiene decenas de extensiones que ejecutan
   y cada programa instalado puede añadir más.
3. **Lista permitida** de formatos de documento, en el núcleo.

## Decisión

**La opción 3.** `--abrir` solo da la ruta si el archivo cumple todo esto; si no, contesta el motivo
fijo nuevo `no_es_documento` y no abre nada:

- Su extensión está en la lista: `pdf`, `txt`, `md`, `csv`, `doc`, `docx`, `odt`, `rtf`, `xls`,
  `xlsx`, `ods`, `pptx`, `odp`, `jpg`, `jpeg`, `png`, `gif`, `webp`, `bmp`, `tif`, `tiff`, `heic`.
  Son los formatos que Brain ya lee o previsualiza, más sus equivalentes de LibreOffice. Fuera
  quedan `html`, `htm`, `svg` y `json`, los formatos con macros (`docm`, `xlsm`) y todo lo demás.
- La extensión es la que va tras el **último** punto del nombre, sin distinguir mayúsculas:
  `factura.PDF.exe` es un `.exe` y `FACTURA.PDF` es un PDF. Un punto o un espacio final (que Windows
  quita) dejan una extensión que no está en la lista.
- El nombre no tiene `:` (en NTFS nombra un flujo alternativo: `programa.exe:x.pdf`) ni caracteres
  de control.
- En el disco es un **archivo normal**, mirado sin seguir enlaces: ni carpeta, ni enlace simbólico
  (abriría su destino, que tiene otro nombre), ni algo que ya no está.

La regla vive en `obrain-api`, junto a las de la 0041, y no en la ventana: `desde_aetheris::ruta`
pide la acción, así que la ventana no puede pedir la ruta de «revelar» y abrirla. El motivo solo sale
después de que el `id` pasó la puerta, así que no dice nada que `--revelar` no diga ya, y la ruta
sigue sin salir.

## Consecuencias

- Lo que no es un documento de la lista no se abre desde Aetheris; se puede enseñar en el
  Explorador con `--revelar`, o abrir desde la ventana de Brain, como siempre.
- Si un formato que JP usa se queda fuera, se amplía la lista (otro cambio con su prueba).
- `doc` y `xls` pueden llevar macros; Office las bloquea por defecto en archivos descargados y pide
  permiso en los demás. Se aceptan porque son documentos comunes de JP.
- Queda una carrera: entre la comprobación y la apertura, alguien con acceso a la carpeta podría
  cambiar el archivo. El programa que se use sigue siendo el de la extensión comprobada.
- Aetheris tiene que conocer el motivo nuevo (`MOTIVOS_BRAIN` en `cliente/ventana.py`) y enseñar un
  texto claro; llega con `git pull`, sin reinstalar Aetheris.
