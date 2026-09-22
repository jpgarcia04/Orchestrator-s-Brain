# Prompt para M5b — «Llegar y ver» · estado al 2026-08-12

> **Cerrado el 2026-08-13.** Las dos piezas están entregadas y el criterio se
> cumple. El informe con lo medido, los tres hallazgos y lo que **no** cubre
> está en [`PROJECT-MASTER.md`](../../PROJECT-MASTER.md) §9. Esto se conserva como
> lo que fue: el alcance con el que se entró, para poder comprobar que no
> creció por el camino.

Copia todo lo de abajo en una conversación nueva de Claude Code, con working directory en este repo.

---

Estoy construyendo **Orchestrator's Brain**, un motor de conocimiento personal local-first, y M5b acaba de **encogerse a propósito**. Lee antes de escribir nada:

1. [`docs/PROJECT-MASTER.md`](../../PROJECT-MASTER.md) — §0 (la tesis), §2 (los invariantes), y sobre todo **«Por qué M5b se encogió»** en §9, que es el contexto de este prompt entero.
2. [`docs/adr/0014`](../../adr/0014-dos-vistas-un-solo-estado.md) — gobierna la mitad de este hito. No es opinable.
3. [`docs/EXPERIENCE.md`](../../EXPERIENCE.md) — §1 (la luz es información: la ley que no se negocia), §4.4 (el panel de entidad) y §4.7 (el conmutador).

## El hito, entero, en una frase

> **Abres la app, llegas hasta un archivo sin escribir su nombre, y lo ves.**

Dos piezas y ninguna más. Si al terminar hay una tercera, el hito volvió a crecer y eso es justo lo que se acaba de corregir.

## Por qué esto y no otra cosa

El motor está terminado hasta M4 y **la interfaz no ha alcanzado nunca al motor**: 15.264 líneas de núcleo contra 1.578 de aplicación. La consulta «comprobante de ingresos» —palabras que no aparecen en el corpus— recupera los estados de cuenta de la visa en 195 ms sobre `%LOCALAPPDATA%\obrain\vida.obrain` (1.042 archivos reales del usuario). Y la app no te deja abrirlos.

El usuario lo dijo con la frase que ordena este hito: *«si no puedo ver los archivos, no va a tener mucho chiste la app»*.

## Pieza 1 — El explorador (ADR-0014)

Hoy **no existe ninguna forma de navegar sin buscar**. La lente necesita un foco y el único modo de conseguirlo es escribir; el cielo se abre o se enciende, pero no tiene pila de navegación.

Lo que hay que construir está decidido en el ADR y aquí sólo va lo que más se rompe al implementarlo:

- **Nunca aparece solo.** Lo pide el usuario con un conmutador visible en sitio fijo *y* un atajo de teclado. En el momento en que aparezca por tocar un nodo, es el modal que §4.4 prohíbe y el ADR deja de sostenerlo.
- **Continuidad de estado en los dos sentidos, y es requisito.** Galaxia → explorador abre en el nodo enfocado con su ruta en las migas; explorador → galaxia deja la cámara donde el explorador te dejó. Si una de las dos direcciones te devuelve al inicio, son dos aplicaciones compartiendo ventana y el usuario elegirá una para siempre.
- **Navega el grafo, no el disco.** Columnas de cualquier relación de contención, no sólo carpetas. Si sólo enseña carpetas, has construido el explorador de Windows con pasos de más.
- **Al entrar, la galaxia se congela**: un fotograma, desenfocado, bucle de render parado (§2, modo reposo).
- **Fuera de esta versión:** arrastrar y redimensionar. Centrado, grande, tamaños preestablecidos.

No hace falta ninguna pieza nueva en el núcleo. `sky::regions`, `sky::cluster`, `Brain::entity` y `Brain::lens` ya dan las dos vistas — la tabla de correspondencia está en el ADR.

## Pieza 2 — El panel de entidad, con previsualización

§4.4 describe el panel (qué es · observado · inferido con su «¿por qué?» · corrección) y nunca dice qué se ve **del archivo**. Eso es lo que falta.

### La trampa, y es la que se paga cara

**14 archivos del corpus son `cloud_dehydrated`: punteros de OneDrive. Leer su contenido los descarga.** M1 se definió entero por no hidratar ni uno, y una miniatura generada al vuelo —o peor, una previsualización al pasar el ratón sobre un nodo— lo violaría en silencio y a escala. Así se pierde de golpe la confianza en un producto que promete no tocar tu nube.

La regla, y no es negociable:

> **Previsualiza sólo lo ya extraído y lo local. Hidratar jamás como efecto secundario — sólo como acto explícito del usuario y dicho con todas las letras.**

**La señal ya existe y ya está en el cable. No la recalcules:**

| Pieza | Dónde | Qué es |
|---|---|---|
| `Availability::may_read_content()` | [`obrain-scan/src/triage.rs`](../../../core/obrain-scan/src/triage.rs) | La guarda. Cierto **sólo** para `Local`. |
| `Availability::parse()` | mismo archivo | Devuelve `None` ante un valor desconocido a propósito: ante la duda, no se toca el archivo. |
| predicado `availability` | log de aserciones | No está en `PLUMBING`, así que `Brain::entity` ya lo devuelve dentro de `observed`. La interfaz **ya lo recibe hoy**. |
| texto extraído (M2) | `chunks` | Ya está guardado. Previsualizar texto no abre ni un archivo. |
| `evidence::pick` | [`obrain-extract/src/evidence.rs`](../../../core/obrain-extract/src/evidence.rs) | Elige qué tramo enseñar. Úsalo también aquí; no vuelvas a recortar por el principio ([ADR-0016](../../adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md)). |

### Tres decisiones distintas, y conviene no confundirlas

1. **Previsualizar dentro de la app** — texto ya extraído, miniatura de imagen local, página de PDF. Es lo que pide §4.1 y lo que hace navegable el mapa.
2. **Abrir con el programa del sistema** — barato en Tauri y probablemente esperado. **Merece un aviso en la interfaz**: obrain es solo-lectura, pero el programa que abra el archivo no tiene por qué serlo. Es solo-lectura *del sistema*, no *del acto*.
3. **Revelar en el explorador de Windows.**

Están sin tomar (§10 del maestro). Pregúntaselas al usuario; no las decidas tú.

## Qué NO hacer

- **No metas nada más en el hito.** Trazo, modo deriva, expansión, tecla global y la dirección visual están **fuera y por escrito** (§9, «Por qué M5b se encogió»). Si crees que algo de eso hace falta para cerrar el criterio, es que el criterio se está reinterpretando.
- **No cierres la dirección visual** (Observatorio vs. Instrumento). Es del usuario y está aplazada a propósito hasta después de este hito. La referencia visual está en [`docs/referencias/`](../../referencias/).
- **No diseñes las regiones creadas por el usuario.** Se movió a M6 (§10).
- **No toques conectores de nube ni de correo.** Aplazado por decisión del usuario; F3 sigue vigente.
- **No previsualices ni generes miniaturas de `cloud_dehydrated` como efecto secundario de nada.**
- **No reabras** el reparto del cielo ([ADR-0015](../../adr/0015-el-cielo-reparte-por-anchura-no-por-peso.md)), el patrón de interacción ([ADR-0014](../../adr/0014-dos-vistas-un-solo-estado.md)) ni la elección de la prueba ([ADR-0016](../../adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md)). Están decididos y medidos.
- **No escribas un ADR por cada arreglo.** El maestro (1.685 líneas) ya es más largo que la aplicación (1.578), y eso es parte del problema que este hito corrige. Un ADR es para una decisión que alguien reabriría dentro de seis meses, no para dejar constancia de que trabajaste.
- **No pulas nada contra `brain.obrain`** — son los 101 archivos de este repo, una muestra de humo. Usa `vida.obrain`.

## Estado del que partes, comprobado y no de memoria

- `cargo test --manifest-path core/Cargo.toml` → **239 en verde**.
- `cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings` → limpio.
- `npx tsc --noEmit` en `ui/` → limpio.
- Corpus real en `%LOCALAPPDATA%\obrain\vida.obrain`: 1.042 archivos, 4.058 fragmentos, 4.026 vectores, 2.394 aristas.
- Herramientas de auditoría ya en el núcleo, de hitos anteriores: `obrain-extract --sample`, `obrain-extract --pruebas`, `obrain-api --pesos`, `obrain-api --cielo`. **Mide con ellas antes de opinar** — es la convención del proyecto y ya ha desmentido dos diagnósticos escritos.

## Y cuando cierre

**Úsalo una semana antes de escribir otra línea.** El proyecto ha medido contra el corpus y contra los invariantes; no ha medido nunca contra la única prueba que queda —si el usuario lo abre solo, sin que nadie se lo pida—, y ésa es la que decide si esto es un producto o una demostración muy bien documentada.
