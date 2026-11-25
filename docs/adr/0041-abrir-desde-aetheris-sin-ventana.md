# ADR-0041 — Abrir, revelar y mostrar un documento a pedido de Aetheris

- **Estado:** Aceptada por JP el 2026-11-25 (pedida desde el proyecto Ecosistema).
- **Fecha:** 2026-11-25
- **Cambia:** nada. Añade tres modos de arranque al lado de `--puerta`
  ([ADR-0038](0038-la-puerta-de-aetheris.md), [ADR-0039](0039-la-puerta-de-aetheris-se-adelanta.md)).
  La puerta, sus herramientas y `SoloNormal` siguen igual.
- **Toca:** `ui/src-tauri/src/main.rs` (los argumentos de arranque y la instancia única) y
  [PUERTA-AETHERIS](../PUERTA-AETHERIS.md).

## Contexto

Cuando Aetheris pregunta a la puerta con `localizar`, cada resultado trae su `id`, su nombre, su
biblioteca, su ruta relativa, su carpeta, su fecha y su tamaño. Con eso JP sabe **qué** documento es,
pero para verlo tiene que abrir Brain, buscarlo otra vez y pulsar «Abrir».

JP pidió el 25 de noviembre de 2026 que en el chat de Aetheris salga una tarjeta del documento y que,
al pulsarla, se vea un panel con sus datos y botones para abrirlo, sin que se abra la ventana de Brain
cada vez. «Ver en Brain» queda como una opción más del panel.

La ventana ya hace las dos cosas que JP quiere: `abrir` (con el programa que Windows tenga asociado) y
`revelar` (abre su carpeta en el Explorador y lo selecciona), las dos a partir del `id` y por
`Brain::path_to_open_version`, que no da la ruta de algo pendiente de identidad, sin contenido legible
o protegido sin el PIN. Falta que otro programa pueda pedirlas.

Que Aetheris abra el archivo por su cuenta no vale: no tiene la ruta completa en `localizar` (es
relativa a la biblioteca a propósito, ADR-0038), y aunque la tuviera, abriría rutas sin las
comprobaciones de Brain. Brain es quien sabe dónde está cada archivo y si se puede abrir.

## Opciones

1. **Nada nuevo.** JP abre Brain y busca. Es lo de hoy, y lo que JP quiere evitar.
2. **Que Aetheris abra la ruta.** Descartada arriba: la ruta completa no cruza la puerta para el
   remoto, y abrirla sería saltarse a Brain.
3. **Una herramienta más en la puerta MCP**, `abrir`. Mezcla un acto de JP con lo que puede pedir un
   modelo: la puerta es de modelos, y un modelo no debe poder abrir un archivo en Word.
4. **Modos de arranque propios, fuera de la puerta**, que solo lanza la ventana de Aetheris cuando JP
   pulsa un botón.

## Decisión

**La opción 4.** `obrain-app.exe` acepta tres modos más, cada uno con un único argumento, un `id`:

| Modo | Qué hace | Ventana de Brain |
|---|---|---|
| `--abrir <id>` | Lo mismo que el botón «Abrir»: el programa que Windows tenga asociado. | No se abre. |
| `--revelar <id>` | Lo mismo que «Revelar en el Explorador». | No se abre. |
| `--mostrar <id>` | Abre la ventana de Brain en la ficha de ese documento, o lleva a ella la que ya está abierta. | Sí. |

### Lo que cumplen los tres

- **Solo lo que la puerta podría servir.** Antes de dar la ruta, el `id` tiene que pasar `SoloNormal`
  (normal, publicación `ready`, ruta vigente, identidad confirmada, fuera de la papelera y sin ser
  copia de algo protegido). Lo demás se rechaza con el mismo motivo fijo que da la puerta, sin decir
  si existe. Para lo sensible, JP lo abre en Brain con su PIN, como siempre. Es más estricto que el
  botón de la ventana, a propósito: quien llama no es la ventana de Brain.
- **Respetan el ajuste de la puerta.** Con la puerta apagada, `--abrir` y `--revelar` se rechazan.
  Con «Sólo localizar» o «Localizar y leer», funcionan: abrir es un acto de JP, no una lectura de un
  modelo, y la ruta completa nunca vuelve a Aetheris.
- **No devuelven la ruta ni el contenido.** La salida es un código y, si falla, un motivo fijo. Nada
  de lo que hay en el archivo cruza.
- **Los mismos avisos que la ventana.** Abrir con el programa asociado puede descargar un archivo que está
  en la nube y el programa puede escribir en él (los dos costes de `abrir`). Brain no los puede decir
  sin ventana, así que lo dice quien pone el botón: Aetheris, en su panel.
- **Se abren para leer.** Igual que la puerta, sin `Brain::open`: no cifran, no migran y no recuperan
  la base. Si la base necesita algo de eso, el modo falla y JP abre Brain.
- **`--mostrar` necesita instancia única.** Si la ventana ya está abierta, el segundo arranque le pasa
  el `id` y termina. Es un plugin de Tauri (`single-instance`) que hoy no está; el `id` que llega así
  se valida igual que los otros dos modos.

## Consecuencias

- JP ve o abre un documento desde Aetheris sin pasar por la ventana de Brain, y Brain sigue siendo
  quien decide qué se abre.
- Un `id` inventado o de algo protegido no abre nada; no se puede usar para saber si existe.
- Hay que construir: el análisis de argumentos, la comprobación con `SoloNormal` para un `id`, la
  instancia única y sus pruebas (un `id` normal abre; sensible, papelera, copia de protegido,
  inventado y puerta apagada no; la salida nunca trae la ruta). Se prueban en Linux con un abridor
  falso y en Windows en la laptop.
- Aetheris necesita su propia decisión para la tarjeta y el panel (su 0017).

## Cuándo se revisa

- Si JP quiere abrir desde Aetheris también lo sensible: entonces el PIN tendría que pedirse en algún
  sitio, y eso es otra decisión.
- Si la puerta cambia `SoloNormal`.
