# La puerta de Aetheris — contrato `1`

Lo que la puerta de Brain contesta, tal como está construida
([ADR-0038](adr/0038-la-puerta-de-aetheris.md),
[ADR-0039](adr/0039-la-puerta-de-aetheris-se-adelanta.md)). Es lo que Aetheris comprueba al
lanzarla y lo que su batería de ataques tiene que probar. Si algo de esto cambia de forma, sube la
versión del contrato.

**Todavía no se conecta:** el modo puerta ya se probó en Windows (2026-11-24, ver
[PROGRESO-CONFIANZA](PROGRESO-CONFIANZA.md)) y la batería de Aetheris pasó contra ella con el modelo
local y con Claude; falta decidir si se conecta.

## Cómo se lanza

```text
obrain-app --puerta
```

Es el mismo ejecutable que instala el instalador de Brain, con `--puerta` como único argumento
(con otro argumento más, sale con código 2). En ese modo no se carga Tauri ni la interfaz, no se
abre ninguna ventana ni corre el actualizador. Habla JSON-RPC 2.0 por stdin y stdout, un mensaje
por línea, y sale con código 0 cuando el cliente le cierra la entrada.

La base es la de `OBRAIN_DB` si está puesta. Si no, la de la carpeta fija de Brain
(`%USERPROFILE%\.obrain\brain.obrain` en Windows). La puerta nunca muda la base desde la carpeta
vieja: si sigue allí, contesta `base_cerrada` hasta que la ventana la mude.

Cada `tools/call` abre la base sólo para leer y la suelta al contestar. Entre llamadas no tiene nada
abierto, y nunca escribe en ella. El modelo de significado, si está descargado, se carga la primera
vez que hace falta y se queda cargado mientras viva el proceso.

## Mensajes

Sólo `initialize`, `ping`, `tools/list` y `tools/call`. Las notificaciones (sin `id`) no se
contestan nunca. Antes de `initialize`, `tools/list` y `tools/call` responden el error `-32002`.

| Caso | Respuesta |
|---|---|
| No es JSON | error `-32700` |
| Un lote, otra cosa que un objeto, `jsonrpc` distinto de `"2.0"`, `id` que no es texto ni entero | error `-32600` |
| Una línea de más de 64 KiB | error `-32600`, sin leerla entera |
| `params` que no es un objeto, argumentos fuera del esquema, herramienta desconocida | error `-32602` |
| Otro método | error `-32601` |

### `initialize`

```json
{
  "protocolVersion": "2025-06-18",
  "capabilities": { "tools": { "listChanged": false } },
  "serverInfo": { "name": "obrain", "title": "Orchestrator's Brain", "version": "1" },
  "instructions": "…",
  "_meta": { "dev.obrain/aplicacion": "0.0.3" }
}
```

`serverInfo.version` es la versión de este contrato. La de la aplicación va aparte, en `_meta`, y
sólo informa. La versión de MCP es la que pidió el cliente, si es `2025-06-18`, `2025-03-26` o
`2024-11-05`; si no, `2025-06-18`.

### `tools/list`

Exactamente `localizar`, `leer` y `documento`, en ese orden, todas con `readOnlyHint: true`,
`destructiveHint: false`, `idempotentHint: true` y `openWorldHint: false`. Los esquemas no admiten
campos de más (`additionalProperties: false`):

| Herramienta | Argumentos |
|---|---|
| `localizar` | `consulta`: texto, de 1 a 500 caracteres y no sólo espacios. `limite`: entero de 1 a 50, por defecto 10. |
| `leer` | Los mismos. |
| `documento` | `id`: texto, de 1 a 64 caracteres. |

## Respuestas de las herramientas

Una respuesta lleva el mismo JSON dos veces: como texto en `content[0].text` y en
`structuredContent`. `isError` es `false`.

- **`localizar`**: `{ "resultados": [Ubicacion], "semantica": bool, "aviso": texto }`. Cada
  `Ubicacion` tiene `id`, `nombre`, `biblioteca` (el nombre de la carpeta, no su ruta), `ruta` y
  `carpeta` relativas a la biblioteca y con `/`, `modificado_ms`, `bytes`, `colecciones` y `ramas`
  (`[["bm25", 1], ["knn", 3]]`). Ningún campo lleva texto del documento.
- **`leer`**: `{ "pasajes": [Pasaje], "semantica": bool, "aviso": texto }`. Cada `Pasaje` tiene
  `documento`, `nombre`, `texto`, `metodo` (`bm25` o `knn`), `leido_ms` (cuándo leyó Brain esa
  versión) y `modificado_ms` (la modificación que vio en disco).
- **`documento`**: `id`, `nombre`, `biblioteca`, `ruta`, `ruta_completa`, `modificado_ms`, `bytes`,
  `colecciones`, `vence` (`null` o `{ valor, metodo, fuente, confianza }`), `relaciones` (`tipo`
  `carpeta`, `copia_de` o `dentro_de`, con `id`, `nombre` y `ruta`) y `aviso`. Sin relaciones
  inferidas en esta versión.

`aviso` es el mismo texto en todas las respuestas, pregunte lo que se pregunte.

### Cuando no puede contestar

Un estado, no un error: `{ "no_disponible": { "motivo": … } }`, con `isError: false`.

| Motivo | Cuándo |
|---|---|
| `apagada` | El interruptor de la ventana no deja esta herramienta: apagada, o `leer` y `documento` en «sólo localizar». |
| `fuera_de_la_puerta` | `documento` con algo que no es un documento normal y publicado: sensible, de bóveda, su copia sin marcar, sin clasificar, en la papelera o inexistente. La misma respuesta en todos los casos. |
| `base_cerrada` | No hay base, está vacía, no tiene llave o la llave no abre. |
| `ocupada` | Una escritura, una restauración, un cifrado o una mudanza a medias, o la base cambió mientras se leía. |
| `version_distinta` | El esquema no es el de esta versión de Brain. |

Un fallo por debajo responde `isError: true` con el texto fijo «Brain no pudo leer la base.», sin
detalles: un mensaje de error puede llevar la ruta de la base y con ella el usuario de Windows.

## Lo que no hay

Ni recursos, ni prompts, ni muestreo, ni escritura, ni PIN, ni puerto, ni red. Lo sensible, lo de
bóveda y lo que está sin clasificar no se consultan, haya PIN o no, esté la ventana desbloqueada o
no. Los nombres de carpetas y colecciones de lo normal sí salen.

## Abrir, revelar y mostrar a pedido de Aetheris

[ADR-0041](adr/0041-abrir-desde-aetheris-sin-ventana.md). Tres modos de arranque más, fuera del
protocolo MCP, para que ningún modelo pueda pedirlos. Los lanza la ventana de Aetheris cuando JP
pulsa un botón de la tarjeta de un documento:

| Orden | Qué hace | Salida |
|---|---|---|
| `obrain-app --abrir <id>` | Lo abre con el programa que el sistema tenga asociado, sin ventana de Brain. | `0` hecho; `1` y una línea con el motivo; `2` y `argumentos` si el pedido está mal formado. |
| `obrain-app --revelar <id>` | Abre su carpeta en el explorador con el archivo elegido, sin ventana de Brain. | Igual. |
| `obrain-app --mostrar <id>` | Abre la ventana de Brain en su ficha, o lleva a ella la que ya está abierta (instancia única). | La ventana. |

- `<id>` es el `id` que dio `localizar`, `leer` o `documento`: un ULID de 26 caracteres. Lo que no
  tiene esa forma no abre la base.
- Sólo lo que la puerta serviría (`SoloNormal`, una ruta vigente dentro de una biblioteca). Para
  todo lo demás, `fuera_de_la_puerta`, sin decir qué es. Con la puerta apagada, `apagada`; «sólo
  localizar» basta, porque abrir es un acto de JP y la ruta no vuelve a Aetheris.
- La base se abre como la abre la puerta, para leer; los motivos de la apertura (`base_cerrada`,
  `ocupada`, `version_distinta`) son los mismos. Si el sistema no puede abrir el archivo,
  `no_se_pudo_abrir`.
- Nunca se escribe la ruta ni nada del contenido. Los avisos de abrir (si está en la nube se
  descarga; el programa puede escribir en él) los pone el botón de Aetheris.

## Dónde está

- `core/obrain-api/src/puerta.rs`: las tres herramientas y el ajuste.
- `core/obrain-api/src/puerta_mcp.rs`: este protocolo.
- `core/obrain-api/src/desde_aetheris.rs`: los pedidos de abrir, revelar y mostrar.
- `ui/src-tauri/src/main.rs`: `--puerta`, `--abrir` y `--revelar`, antes de Tauri; `--mostrar`, con la
  ventana.
- Pruebas: `core/obrain-api/tests/puerta.rs` y `ui/src-tauri/tests/puerta.rs` (el ejecutable de
  verdad, sin pantalla).
- La base de la batería de Aetheris: `core/obrain-api/tests/bateria_aetheris.rs`, que sólo corre
  con `--ignored` y `OBRAIN_BATERIA` (lo lanza `pruebas/ataques/preparar_brain.py` de Aetheris).
