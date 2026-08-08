# Orchestrator's Brain — Documento Maestro

> Las aplicaciones actuales almacenan información. Orchestrator's Brain construye conocimiento.

**Estado:** Fase 2 — Arquitectura · **Última revisión:** 2026-08-07 · **Documento vivo**

Este documento refleja siempre el estado *actual* del diseño. Se edita libremente.
El *porqué* de las decisiones contestadas vive en [`adr/`](adr/) y no se edita nunca.

---

## 0. Tesis y métrica de gobierno

Un sistema **construye** conocimiento si una parte medible del grafo **no fue escrita por el usuario**: entidades inferidas, relaciones descubiertas, procesos reconstruidos, contradicciones detectadas.

**Métrica que gobierna el producto:**

> `% de aserciones del grafo autoradas por el sistema, ponderadas por las que el usuario no retracta`

El segundo término importa tanto como el primero: generar mucho y equivocarse no es construir conocimiento, es generar trabajo de limpieza. Esta métrica es el filtro para decir "no" a features durante los próximos años.

**Test de aceptación canónico de la v1:**

> *"¿Dónde estaba el documento que usé para demostrar ingresos durante mi trámite de visa?"*

Esa frase no contiene el nombre del archivo, ni su ruta, ni su fecha. Contiene una **relación** (documento ↔ trámite), un **rol** (demostrar ingresos) y un **periodo** (durante). Si el sistema la responde, la arquitectura es correcta.

---

## 1. Decisiones fundacionales (fijadas — cambiar una invalida el resto)

| # | Eje | Decisión | Consecuencia dura |
|---|-----|----------|-------------------|
| F1 | Datos y cómputo | **Local-first** + sync cifrado opcional | El servidor nunca ve contenido en claro. No puede embeber ni razonar. |
| F2 | Ambición | **Personal primero, comercial-ready** | Multi-tenant, permisos, temporalidad y auditoría existen en el *modelo* desde el día uno. |
| F3 | Cuña v1 | **"Encuentra lo que no recuerdas que tienes"** | Sin conectores de terceros en v1. |
| F4 | Plataforma | **Escritorio Windows primero** | La web es un cliente futuro del mismo núcleo, nunca el núcleo. |

## 2. Invariantes de diseño

Reglas que cualquier propuesta debe respetar. Son las decisiones caras o imposibles de corregir más tarde.

| # | Invariante | Motivo |
|---|-----------|--------|
| I1 | **El log de aserciones es la verdad. Todo índice es una proyección reconstruible.** | Permite migrar embeddings, cambiar motor de búsqueda y reconstruir el grafo sin perder datos. |
| I2 | **El grafo es una lente sobre una consulta, nunca un volcado.** Foco + radio + presupuesto duro de nodos. | La visualización sólo funciona entre ~50 y ~500 nodos. El corpus real son cientos de miles. |
| I3 | **La similitud semántica no se materializa como arista.** Es operación de consulta. | Persistirla convierte el grafo en niebla en seis meses. |
| I4 | **Nada se borra. Se retracta.** | Es lo que hace real el "recordar". Y da la auditoría gratis. |
| I5 | **Las aserciones del usuario tienen precedencia permanente sobre cualquier inferencia.** | Si un reescaneo revierte una corrección, se pierde la confianza para siempre. |
| I6 | **Bitemporalidad**: cuándo fue cierto ≠ cuándo el sistema se enteró. | Barato ahora, imposible de retrofitear. |
| I7 | **Todo vector guarda `modelo + versión + dimensión + chunker`.** Dos índices en paralelo durante migraciones. | Los embeddings son el lock-in real, no el LLM. |
| I8 | **La identidad de un archivo no es su ruta**, es `(volumen, FileID) + hash`. | Sobrevive a mover, renombrar y desconectar discos. |
| I9 | **Ninguna respuesta sin procedencia.** El sistema debe poder decir "no lo sé". | Una ubicación inventada vale menos que ninguna respuesta. |
| I10 | **La animación jamás está en el camino crítico de la información.** | La sensación premium es latencia, no shaders. |
| I11 | **Egreso explícito**: cada entidad tiene nivel de sensibilidad; cada capacidad de IA declara si puede salir de la máquina. | Es lo que sostiene honestamente "guarda aquí tu pasaporte". |

---

## 3. Modelo de datos

### 3.1 La unidad fundamental no es el nodo: es la aserción

La mayoría de sistemas de conocimiento hacen del **nodo** la unidad primaria y luego intentan colgarle procedencia, confianza y tiempo. Sale mal: acabas con columnas `created_by`, `is_ai_generated`, `confidence` esparcidas y sin semántica consistente.

Aquí la unidad primaria es la **Aserción**: *alguien afirmó algo, en un momento, con una confianza, basándose en una evidencia*. Nodos y aristas son **proyecciones** del log de aserciones.

```
Assertion {
  id                : ULID                     -- ordenable por tiempo
  subject           : EntityId
  predicate         : PredicateId              -- vocabulario controlado
  object            : EntityId | Literal

  -- Bitemporalidad (I6)
  valid_from        : Timestamp?               -- cuándo es/fue cierto en el mundo
  valid_to          : Timestamp?               -- null = sigue siendo cierto
  asserted_at       : Timestamp                -- cuándo lo supo el sistema
  retracted_at      : Timestamp?               -- I4: nunca se borra

  -- Procedencia (I9)
  method            : DETERMINISTIC | INFERRED | USER | IMPORTED
  source            : SourceRef                -- qué escaneo / qué modelo / qué acción
  confidence        : Float                    -- 1.0 para DETERMINISTIC y USER
  evidence          : [EvidenceSpan]           -- offset exacto en el documento origen
}
```

Consecuencias que salen **gratis** de esta forma, y que serían features caras en cualquier otro modelo:

- **Auditoría** — el log *es* el registro de auditoría.
- **Deshacer** — retractar es una operación normal.
- **"¿Por qué crees eso?"** — `evidence` + `source` responden literalmente.
- **Sincronización** — replicar un log append-only de entradas inmutables es mucho más fácil que reconciliar estado (§8.2).
- **Detección de contradicciones** — dos aserciones vivas sobre el mismo `(subject, predicate)` con valores incompatibles *son* una contradicción, detectable con una consulta.
- **"¿Qué sabía yo en marzo de 2023?"** — filtrar por `asserted_at`.

### 3.2 Entidades: tipos por composición de facetas

El requisito "aceptar nuevos tipos sin romper la arquitectura" descarta el esquema fijo por tipo. Pero *schemaless* puro no da ninguna palanca: ni validación, ni UI generada, ni extractores especializados.

**Una entidad es un ID + un conjunto de facetas.** El "tipo" es una etiqueta de presentación; el comportamiento cuelga de las facetas.

| Faceta | Aporta | Ejemplos que la llevan |
|--------|--------|------------------------|
| `Addressable` | Identidad física: hash, FileID, rutas observadas | Documento, foto, vídeo |
| `Textual` | Contenido extraíble, chunks, índice léxico y vectorial | PDF, nota, correo, código |
| `Temporal` | Instante o intervalo | Evento, proceso, viaje |
| `Agent` | Actúa: autoría, envío, participación | Persona, organización |
| `Geolocated` | Coordenadas | Foto con EXIF, viaje |
| `Financial` | Importe, moneda, contraparte | Factura, recibo |
| `Process` | Objetivo, estado, participantes, artefactos | Trámite, proyecto, mudanza |
| `Confidential` | Nivel de sensibilidad y política de egreso | Pasaporte, credencial |

Una foto = `Addressable + Temporal + Geolocated`.
Una factura = `Addressable + Textual + Temporal + Financial`.
Un trámite de visa = `Process + Temporal`.

Añadir "Curso" o "Libro" en el futuro es una combinación nueva, no una migración.

### 3.3 `Process`: la entidad que responde la pregunta de la visa

Merece mención aparte porque es la clave de la cuña.

Un `Process` es una secuencia acotada y orientada a un objetivo, con estado, periodo, participantes y artefactos: *trámite de visa*, *mudanza*, *compra de piso*, *un proyecto*, *un tratamiento médico*.

La pregunta *"durante mi trámite de visa"* no es una búsqueda de texto. Es:
1. resolver "trámite de visa" a una **entidad** `Process`,
2. leer su **intervalo temporal**,
3. recorrer sus **artefactos** filtrando por rol (`evidence_for` → "ingresos").

Y lo importante: **este nodo debería crearlo el sistema, no el usuario.** Un clúster de PDFs + correos + fechas convergiendo hacia un resultado es detectable. Un `Process` autoinferido es el ejemplo más puro de "construye conocimiento" — y es exactamente lo que mide la métrica de §0.

### 3.4 Predicados: vocabulario controlado

Las aristas con texto libre matan el grafo (`worked_on` / `workedOn` / `trabajó_en` → inconsultable). Cada predicado se declara:

```
Predicate {
  id, label
  domain, range     : FacetRequirement
  cardinality       : ONE | MANY
  symmetric         : Bool          -- duplicate_of
  transitive        : Bool          -- contains
  inverse           : PredicateId?  -- contains <-> contained_by (no se almacenan dos veces)
  default_method    : DETERMINISTIC | INFERRED
}
```

Vocabulario inicial, agrupado por **cómo se obtiene** — que es lo que determina su coste y su fiabilidad:

- **Estructural** (determinista): `contains`, `derived_from`, `version_of`, `duplicate_of`, `attached_to`
- **Temporal** (determinista): `occurred_during`, `precedes`, `supersedes`
- **Social** (determinista): `authored_by`, `sent_to`, `mentions`
- **Semántico** (inferido): `about`, `part_of_process`, `evidence_for`, `contradicts`

El último grupo es donde vive el valor diferencial. Los tres primeros son donde vive el **80% del valor inicial**, y no necesitan IA (§4.4).

---

## 4. Ingestión

Pipeline: **Descubrir → Identificar → Triar → Extraer → Enriquecer → Aseverar → Indexar**

### 4.1 Descubrir

Escaneo inicial + vigilancia continua. En Windows la vía correcta para el incremental sobre un volumen completo es el **USN Change Journal** (es lo que hace que Everything sea instantáneo), con `ReadDirectoryChangesW` para carpetas concretas.

Exclusiones por defecto, agresivas: `node_modules`, internos de `.git`, `AppData`, `Program Files`, cachés, entornos virtuales, `$Recycle.Bin`. Un escaneo ingenuo de una máquina de desarrollo indexa 400.000 archivos basura antes de llegar a un solo documento útil.

### 4.2 Identificar

Identidad = `(GUID de volumen, FileID de NTFS)` + `BLAKE3` del contenido. La **ruta es una observación temporal**, no la clave (I8). Mover o renombrar un archivo no crea una entidad nueva; añade una aserción `path_observed_at`.

### 4.3 Triar — la etapa que casi nadie implementa y que en Windows es crítica

**Placeholders de OneDrive / Files On-Demand.** Muchos archivos en disco **no tienen bytes**. Abrirlos para hashear o extraer texto **dispara la hidratación**, es decir, la descarga. Un indexado ingenuo de `Documentos` puede bajar decenas de GB, llenar el disco y saturar la red.

Detección **antes** de abrir, vía atributos: `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`, `FILE_ATTRIBUTE_RECALL_ON_OPEN`, `FILE_ATTRIBUTE_OFFLINE`.

Política: los placeholders se indexan **sólo por metadatos** y se encolan. La hidratación requiere consentimiento explícito y respeta un presupuesto de tamaño y de red.

Este proyecto vive en `OneDrive\Documentos\`. No es un caso hipotético.

**Bonus determinista de Windows:** los archivos descargados llevan un flujo alternativo NTFS `:Zone.Identifier` con la **URL de origen**. Es una arista `downloaded_from` gratuita, fiable y casi nadie la usa. Conecta un PDF con la web de donde salió, y de ahí con el correo o el trámite.

### 4.4 Enriquecer — deterministas primero, y por atención después

**Aristas deterministas disponibles sin una sola llamada a IA:**

| Señal | Arista que produce |
|-------|--------------------|
| Hash idéntico | `duplicate_of` |
| Proximidad de `mtime`/`btime` | archivos tocados en la misma sesión de trabajo — señal sorprendentemente fuerte |
| EXIF (fecha, GPS, cámara) | `occurred_during`, `Geolocated` |
| Metadatos PDF/Office (autor, creación, app) | `authored_by`, `Temporal` |
| Nombre de A citado dentro de B | `mentions` |
| Estructura de carpetas y hermanos | `contains`, candidatos a `part_of_process` |
| `:Zone.Identifier` | `downloaded_from` |
| Git (commits, autores, fechas) | `authored_by`, `Temporal` |

**Enriquecimiento dirigido por atención.** No se puede pasar un LLM por 200.000 archivos: ni económicamente, ni térmicamente, ni en tiempo. La inferencia va en una **cola diferida con presupuesto**, priorizada por atención del usuario: abierto recientemente, dentro de un foco activo, tocado por una búsqueda, vecino de algo que sí importa.

Esto no es sólo una necesidad económica. Es cognitivamente correcto: **el sistema entiende en profundidad lo que tocas, no todo por igual.** Igual que un cerebro.

---

## 5. Capa de IA

### 5.1 Abstracción por capacidad, no por API

Abstraer a nivel de `chat()` / `complete()` es una abstracción que gotea: los modelos difieren radicalmente en tool use, salida estructurada y ventana de contexto. El contrato se define por **capacidad**:

| Capacidad | Firma | Local viable | Puede salir |
|-----------|-------|--------------|-------------|
| `Embedder` | `text[] → vector[]` | Sí (modelos pequeños en CPU) | Configurable |
| `Extractor` | `doc → Entity[] + Assertion[]` | Parcial | Según sensibilidad |
| `Classifier` | `doc → facets/labels` | Sí | Según sensibilidad |
| `Summarizer` | `doc \| cluster → text` | Degradado | Según sensibilidad |
| `Answerer` | `question + evidence → answer + citations` | Depende del hardware | Según sensibilidad |
| `Reranker` | `query + candidates → ordered` | Sí (cross-encoder pequeño) | No hace falta |

Cada capacidad lleva **dataset dorado** y modelo de coste. Es lo que permite afirmar con datos "el proveedor X es mejor aquí", que es la intercambiabilidad real — no un adaptador lleno de `if provider ==`.

### 5.2 Política de egreso (I11) → [`core/obrain-guard`](../core/obrain-guard)

Cada entidad tiene un nivel de sensibilidad; cada invocación declara si puede salir de la máquina. El router **rechaza** enviar una entidad `vault` a un proveedor remoto. No es una preferencia de configuración: es una comprobación aplicada en la capa de routing y verificable con tests.

| Nivel | Índice vectorial | Egreso a modelo remoto | Sync remoto |
|-------|------------------|------------------------|-------------|
| `normal` | Sí | Sí, con consentimiento | Contenido cifrado |
| `sensitive` | Sí, local | Sólo modelos locales | Contenido cifrado |
| `vault` | Sólo léxico local | Nunca | Sólo metadatos |

**Implementada (2026-08-07)**, antes de que exista un solo vector. `gate::decide` es esta tabla como función pura; sus tests son la tabla. Falla cerrado: una entidad desconocida se trata como `vault`. La búsqueda léxica queda deliberadamente fuera de la puerta —la tabla ya se la concede a `vault`— porque BM25 es determinista, local y no deja un derivado del que se pueda reconstruir el original. Un vector sí, y por eso sí pasa. El nivel se clasifica con señales de nombre y de contenido, y vive según [ADR-0006](adr/0006-donde-vive-la-sensibilidad.md). Ver el informe abajo.

### 5.3 Migración de embeddings (I7)

Todo vector guarda `(model_id, model_version, dim, chunker_version)`. Una migración es: crear el índice nuevo en paralelo, re-embeber en segundo plano con presupuesto, conmutar cuando la cobertura sea suficiente, retirar el viejo. Sin esto, cambiar de modelo de embeddings a los dos años es un evento de extinción.

**Implementado (2026-08-07)** por [`core/obrain-embed`](../core/obrain-embed). Los cinco datos están **en la clave primaria** de `vectors`, que es lo que hace que «en paralelo» sea posible y no una intención: con la clave puesta en el fragmento, el segundo modelo pisaría al primero y la migración dejaría de ser gradual. Ver [ADR-0009](adr/0009-donde-viven-los-vectores.md) y el informe de M4 abajo.

---

## 6. Búsqueda y recuperación

Donde se responde la pregunta de la visa.

```
1. Comprensión de consulta
   ├── términos léxicos
   ├── intención semántica
   ├── restricciones temporales   "durante mi trámite de visa"
   │       └── resolver a entidad Process → leer su intervalo
   └── restricciones de entidad / rol   "demostrar ingresos" → evidence_for

2. Recuperación en paralelo
   ├── BM25            (índice léxico)
   ├── kNN vectorial   (índice semántico)
   ├── travesía de grafo desde entidades resueltas (profundidad acotada)
   └── filtro temporal (bitemporal)

3. Fusión — Reciprocal Rank Fusion
      Sin pesos que ajustar a mano, robusta frente a escalas incomparables.

4. Rerank — cross-encoder local sobre el top-50

5. Respuesta con citas — o "no lo sé" (I9)
```

El punto clave: **"durante mi trámite de visa" no es una coincidencia de texto.** Es resolución de entidad + extracción de rango temporal. Por eso `Process` y la bitemporalidad son estructurales y no adornos.

---

## 7. Stack

Restringido por: escritorio Windows, local-first, BD embebida, indexado en segundo plano, E/S de archivos intensiva, UI de clase WebGL, inferencia local.

### 7.1 Persistencia: SQLite como sustrato único → [ADR-0002](adr/0002-sustrato-de-persistencia.md)

La respuesta ingenua es "Neo4j + Elasticsearch + una BD vectorial". Es la respuesta equivocada para local-first: tres servidores que instalar, tres problemas de sincronización, tres modelos de consistencia y 2 GB de RAM antes de indexar nada.

| Capa | Implementación |
|------|----------------|
| Log de aserciones | Tabla SQLite append-only — **la verdad** (I1) |
| Grafo | Tablas de adyacencia + CTEs recursivas — **proyección** |
| Texto completo | FTS5 (BM25, integrado) — **proyección** |
| Vectores | tabla propia + recorrido lineal exacto — **proyección** ([ADR-0010](adr/0010-recorrido-lineal-frente-a-indice-ann.md)) |
| Blobs | Almacén direccionado por hash en disco |
| Cifrado | SQLCipher en reposo |

Un archivo. Cero instalación. Copia de seguridad atómica. Y como todo índice es reconstruible desde el log, cambiar cualquiera de ellos no es una migración: es un `rebuild`.

*Sobre el rendimiento del grafo:* las CTEs recursivas de SQLite son perfectamente adecuadas para travesías de **profundidad acotada**, que es lo único que hacemos — el grafo es una lente con radio (I2). No calculamos PageRank global. Si a 5M+ aristas hiciera falta, se añade un motor de grafo embebido como **proyección derivada** (una caché, no una segunda fuente de verdad).

### 7.2 Núcleo de indexación: Rust → [ADR-0003](adr/0003-lenguaje-del-nucleo.md)

- Escanear y hashear cientos de miles de archivos está limitado por E/S y paralelismo. Rust da hilos reales, sin pausas de GC y con memoria predecible. Una pausa de recolector durante un escaneo de 300.000 archivos es una UI que tartamudea.
- El consumo de un demonio que corre todo el día es decisivo. Un proceso en reposo a 400 MB es un motivo de desinstalación.
- Ecosistema real hoy: `rusqlite`, `tantivy`, `candle`/`ort`, `notify`, `blake3`, `tokio`.
- Binario nativo único: no hay runtime que instalar.

**Por qué no Java** (pregunta explícita): arranque de JVM, línea base de memoria y empaquetar un JRE en una app de escritorio son costes reales, y ninguna de las fortalezas de Java —ecosistema servidor maduro, mantenibilidad en equipos grandes, throughput del JIT en servicios de larga vida— aplica a un demonio local monousuario. Java sería defendible para el relay de sincronización (§8.2). Es la elección equivocada para el núcleo.

**Por qué no Node en el núcleo:** el bucle de eventos monohilo es lo contrario de lo que necesita una carga CPU-bound de extracción y hashing. `worker_threads` es un parche, no un diseño. Node encaja como pegamento, no como motor.

**Por qué no Go:** es la alternativa honestamente defendible, y lo digo sin adornos. Más simple que Rust, buena concurrencia, binario único. Elijo Rust por las pausas de GC durante escaneos grandes, un ecosistema de ML local más débil en Go, y las garantías de propiedad en la mutación concurrente del índice, que haremos constantemente. Es la única decisión de stack donde la opción descartada sigue siendo razonable.

### 7.3 UI: Tauri v2 + TypeScript

- Frente a Electron: ~10 MB contra ~150 MB, y usa el webview del sistema. Para una app permanentemente abierta, la diferencia de memoria decide.
- **Coste honesto:** Tauri usa WebView2 (Chromium) en Windows pero WebKit en Linux/macOS, con lo que el renderizado difiere entre plataformas. Como la v1 es sólo Windows, el coste está diferido — pero está, y hay que decirlo ahora y no descubrirlo el día del port.
- Frente a nativo (WinUI 3): mejor techo de rendimiento, pero renuncia a todo el ecosistema web de visualización, que es justo lo que necesita la galaxia. No compensa.

### 7.4 Visualización del grafo

Como la lente acota los nodos visibles a ~500 (I2), **no necesitamos un renderizador de 100.000 nodos. Necesitamos uno bellísimo de 500.** Eso cambia el criterio: de "cuál escala" a "cuál da más control visual".

Capa propia sobre **PixiJS** (WebGL2, maduro) con simulación de fuerzas en un worker.

Y un requisito que casi todos los grafos de fuerzas incumplen, y que es la razón por la que se sienten juguetes: **el layout tiene que ser estable.** Misma consulta = mismas posiciones. Semilla determinista + posiciones persistidas por entidad. La memoria espacial sólo funciona si las cosas no saltan de sitio.

### 7.5 3D

2D con profundidad simulada (parallax, bloom, desenfoque por distancia) como vista de trabajo: se ve vivo y se **lee**. El 3D real queda reservado a modos donde el eje tenga semántica — navegación temporal, palacio de memoria. En un grafo de conceptos, la oclusión y la ambigüedad de profundidad hacen el 3D objetivamente peor para leer topología.

---

## 8. Seguridad y sincronización

### 8.1 Seguridad (diseñada ahora, implementada por etapas)

- **Niveles de sensibilidad** por entidad: `normal` / `sensitive` / `vault` (§5.2).
- **`vault`**: clave separada derivada de una frase de paso que no se almacena. Excluido de egreso, del índice vectorial y del contenido en el sync remoto.
- **Claves**: DPAPI / Credential Manager de Windows para la clave cotidiana; derivación por frase de paso para la bóveda.
- **En reposo**: SQLCipher.
- **Auditoría**: gratis. El log de aserciones append-only *es* el registro de auditoría (I4).

### 8.2 Sincronización

Aquí es donde el modelo de §3.1 se paga solo.

Como el log es **append-only** y cada aserción es **inmutable y direccionable por contenido**, sincronizar es **replicar un log**, no reconciliar estado. Es un problema de un orden de magnitud menor.

Los conflictos sólo aparecen entre aserciones vivas y competidoras sobre el mismo `(subject, predicate, tiempo de validez)`. Se resuelven por precedencia de método (USER gana siempre, I5) y, en empate, se elevan al usuario como una contradicción — que ya es un concepto de primera clase del sistema.

El relay remoto sólo ve blobs cifrados y no puede leer nada (F1).

---

## 9. Roadmap

Cada hito se valida contra la pregunta de la visa.

| Hito | Contenido | Criterio de "hecho" |
|------|-----------|---------------------|
| ✅ **M0 — Esqueleto** | Log de aserciones, esquema, facetas, predicados. Sin UI. → [`core/obrain-core`](../core/obrain-core) | **Hecho (2026-08-07).** 14 tests + doctest en verde, clippy limpio con `-D warnings`. I1 e I4 impuestos por triggers de SQLite. |
| ✅ **M1 — Ver el disco** | Descubrimiento, identidad, triaje de placeholders, exclusiones. → [`core/obrain-scan`](../core/obrain-scan) | **Hecho (2026-08-07).** Ver informe abajo. |
| ✅ **M2 — Leer el disco** | Extracción por formato, chunking versionado, FTS5. → [`core/obrain-extract`](../core/obrain-extract) | **Hecho (2026-08-07).** 5.015 fragmentos, búsqueda BM25 en 0,5–1,9 ms. Ver informe abajo. |
| ✅ **M3 — Relacionar sin IA** | Aristas deterministas de §4.4. → [`core/obrain-link`](../core/obrain-link) | **Hecho (2026-08-07).** 2.480 aristas, 0 llamadas a un modelo. Ver informe abajo. |
| 🔒 **Puerta de sensibilidad** | Clasificación `normal`/`sensitive`/`vault` y aplicación de §5.2. → [`core/obrain-guard`](../core/obrain-guard) | **Hecha (2026-08-07).** No es un hito: es el requisito previo de M4. Ver informe abajo. |
| ✅ **M4 — Entender** | Embeddings locales, búsqueda semántica, fusión RRF. → [`core/obrain-embed`](../core/obrain-embed) | **Hecho (2026-08-07).** 2.479 vectores, `universidad` encuentra un formulario de matrícula que no contiene la palabra. Rerank **diferido con la medición** ([ADR-0011](adr/0011-rerank-diferido.md)). Ver informe abajo. |
| **M5 — La galaxia** | Lente, layout estable, navegación por foco. | Se puede explorar un `Process` y sus artefactos visualmente. |
| **M6 — Construir conocimiento** | Extracción de entidades, detección de `Process`, enriquecimiento por atención. | La métrica de §0 es medible y positiva. |
| **M7 — La pregunta** | Comprensión de consulta, resolución temporal, respuesta con citas. | **Responde la pregunta de la visa.** |

M3 es el hito más subestimado y probablemente el que más valor por hora de trabajo entrega. M7 es el que valida la tesis.

### Informe de M1 — escaneo real (2026-08-07)

`obrain-scan C:\Users\pacox\OneDrive --verify`, build release.

| | |
|---|---|
| Archivos en disco bajo la raíz | 35.583 |
| Bajo directorio podado (nunca enumerados) | 33.517 |
| **Archivos vistos** | **2.066** ← la aritmética cierra exactamente |
| Directorios podados | 12 (5 build, 6 herramienta, 1 oculto) |
| Deshidratados de nube detectados | 14 |
| **Hidratados por el escaneo** | **0** ✓ |
| Leídos para hashear | 2.045 (638,7 MB) |
| Duración del recorrido | 0,11 s |
| Aserciones escritas | 14.913 |

**Idempotencia** (segundo escaneo sobre carpeta estática): 0 entidades nuevas,
0 aserciones escritas, 36 sin cambio. La identidad `(volumen, FileID)` reconoce
los archivos ya vistos y el log **no se infla al reescanear** — que era la
preocupación abierta en [ADR-0001](adr/0001-log-de-aserciones-como-fuente-de-verdad.md).

**Lo que este informe NO cubre:**
- Sólo se probó `RECALL_ON_DATA_ACCESS`. **No había ni un archivo
  `RECALL_ON_OPEN`** en la máquina, así que esa rama —la más peligrosa, donde
  abrir ya descarga— está implementada y cubierta por tests unitarios, pero
  **no verificada contra el sistema real**.
- No se ha probado sobre un volumen completo (`C:\`), ni con discos externos,
  ni con rutas de más de 260 caracteres.
- El USN Change Journal no está implementado: el recorrido es completo cada vez.
  Para M1 basta; para vigilancia continua hará falta.

### Informe de M3 — enlazado real (2026-08-07)

`obrain-link` sobre el corpus de 1.291 archivos. **Cero llamadas a un modelo.**

| Arista | Método | Nº |
|---|---|---|
| `contained_by` | determinista | 1.523 |
| `occurred_during` (sesiones) | heurística, conf 0,70 | 894 |
| `duplicate_of` | determinista, conf 1,00 | 37 |
| `downloaded_from` | determinista, conf 1,00 | 24 |
| `version_of` | heurística, conf 0,75 | 2 |
| **Total** | | **2.480** |

353 nodos concentradores creados —carpetas y sesiones— que **nadie escribió**.
Densidad: 1,92 aristas por archivo.

**`downloaded_from` es el hallazgo del hito.** Reconstruye la procedencia web de
documentos reales de trámites desde un flujo NTFS que lleva veinte años ahí y que
casi nadie usa. Es exactamente la arista que la pregunta de la visa necesita.

### Lo que enseñó auditar el grafo con `--sample`

Los contadores del primer enlazado decían «4.336 aristas» y parecían un éxito.
Mirar aristas reales destapó **tres defectos que ningún test unitario tenía**:

| Defecto | Antes | Después |
|---|---|---|
| Fotos `IMG_0001…` encadenadas como versiones | 369 `version_of` | regla del marcador inequívoco |
| Series numeradas (`Bloque 3` → `Bloque 5`) tratadas como versiones | 261 | regla «original + numerados» → **2** |
| Todos los archivos de 0 bytes enlazados entre sí (mismo hash) | 127 `duplicate_of` | se excluyen → **37** |

Y una cuarta, en M1: la mitad del corpus eran **cachés de Chromium y partidas
guardadas que las aplicaciones vuelcan dentro de `Documentos`**. Ningún filtro por
extensión las habría cogido. Se añadió `APP_DATA_DIRS` (29 directorios podados,
2.066 → 1.298 archivos).

**Conclusión metodológica:** un enlazador que no se puede inspeccionar es un
enlazador en el que no se puede confiar. `--sample` pasa a ser parte del hito,
no una herramienta de depuración.

**Problema abierto:** la exclusión por lista fija tiene techo. Todavía entran
`MANIFEST-000001`, `preloaded_data.pb` y listas de palabras de alguna aplicación.
Hace falta un **clasificador de "esto es dato de aplicación"** basado en señales
(nombres no legibles, extensiones opacas, densidad de archivos por carpeta), no
una lista que crece para siempre.

### Informe de M2 — extracción real (2026-08-07)

`obrain-extract` sobre la base que dejaron M1 y M3, build release.

**Decidido sin abrir un solo archivo** — la etapa que hereda entera la regla de M1:

| | |
|---|---|
| Archivos en el log | 1.292 |
| Formato no soportado (fotos, vídeo, binarios) | 818 |
| **No local — no se tocó** | **14** |
| Más grandes de 32 MB | 0 |
| **Archivos abiertos** | **460** ← 818 + 14 + 0 + 460 = 1.292 |

*(1.292 y no los 1.291 del informe de M3: el propio repositorio ha ganado un
archivo desde entonces. Los números de M2 salen de un escaneo nuevo, no del que
midió M3.)*

**De los 460 que sí se abrieron:**

| | |
|---|---|
| Texto extraído | 432 |
| Sin texto (vacíos o escaneados) | 6 |
| Binario disfrazado de texto | 9 |
| Ruido descartado (minificado / diccionario) | 13 |
| Falló el extractor | 0 |
| | 432 + 6 + 9 + 13 + 0 = **460** ✓ |

| Índice léxico | |
|---|---|
| Fragmentos | 5.015 |
| Caracteres de texto | 8.908.504 |
| Caracteres guardados (con solape) | 9.772.967 → **9,7 % de solape** |
| Media | 11,6 fragmentos por archivo |
| Bytes leídos de disco | 21,0 MB |
| Duración | 2,68 s |
| Base de datos | 4,28 MB → 23,15 MB |

| Por formato | archivos | fragmentos | caracteres |
|---|---|---|---|
| plaintext | 420 | 4.901 | 8.809.413 |
| html | 35 | 77 | 67.925 |
| pdf | 5 | 37 | 31.166 |
| ooxml | **0** | — | — |

**Búsqueda léxica** — el criterio de "hecho" del hito: **0,5–1,9 ms** por consulta
sobre el corpus completo. Buscar `expediente` devuelve
`consultaExpte_<NIE>_…pdf`: la palabra no está en el nombre del archivo, sólo
dentro. Es la primera vez que el sistema encuentra algo que el usuario no había
escrito en ningún sitio.

> El nombre real de ese expediente llevaba un NIE dentro y estaba citado
> literalmente aquí y en `obrain-extract/src/search.rs`. Lo destapó la primera
> pasada del clasificador de sensibilidad; ver el informe de la puerta más
> abajo. Queda redactado en el árbol de trabajo, **no en el historial de git**.

**Idempotencia:** segunda ejecución → 460 sin cambio, **0 archivos abiertos, 0
aserciones escritas, 0,00 s.** El hash del archivo decide si hay que volver a
extraer; el hash del texto, si hay que volver a fragmentar.

**El log crece con lo que no se puede recalcular, y sólo con eso.** 2.620
aserciones: 432 extracciones correctas × 6 (`extraction_status`, `text_chars`,
`chunk_count`, `text_hash`, `chunker`, `has_facet=textual`) + 28 × 1 para vacíos,
binarios y ruido. `unsupported` y `not_local` **no** escriben nada: se deducen de
la ruta y de la aserción `availability` que ya escribió M1. El texto no está en el
log — vive en `chunks`, que es una proyección ([ADR-0005](adr/0005-donde-vive-el-texto-extraido.md)).

### El hallazgo del hito: los 14 archivos que no se tocaron son los 14 documentos de Office

Los 14 archivos deshidratados de OneDrive que detectó M1 son **exactamente** los
14 documentos de Office del corpus: 10 `.docx`, 2 `.xlsx`, 2 `.pptx`. Presupuestos,
actas, revisiones de almacén, flujogramas comerciales — el material más parecido a
la pregunta de la visa que hay en el disco.

Extraer su texto exigiría descargarlos. Así que **el extractor de OOXML se escribió,
se probó con material sintético y no ha corrido nunca contra un archivo real.**
Mismo estado exacto que `RECALL_ON_OPEN` en M1: implementado, cubierto por tests,
no verificado contra la realidad. Se dice aquí en vez de esconderlo en un `0` de
la tabla.

Es también la mejor demostración posible de por qué el triaje va antes de abrir:
el caso peligroso no era hipotético, era el 100 % de los documentos ofimáticos.

### Lo que enseñó auditar las extracciones

Tres defectos, ninguno visible en los contadores:

| Defecto | Cómo se vio | Antes | Después |
|---|---|---|---|
| `\0` es UTF-8 válido, así que la validación pasaba **antes** que el olfateo de binario | test unitario en rojo | todo LevelDB de Chromium indexable | olfateo primero |
| `LOG` y `LOCK` no tienen extensión, pero el nombre entero se usaba como extensión | lista de "abiertos y sin texto" | 518 archivos abiertos | **460** |
| 12 archivos ocupaban el **62,5 %** del índice y ninguno lo había escrito una persona | `--sample`, sección "quién ocupa el índice" | 6.412 fragmentos | **5.015** (−21,8 %) |

El tercero motivó [`noise.rs`](../core/obrain-extract/src/noise.rs), la primera
aportación al *clasificador de dato de aplicación* que M1 dejó como problema
abierto. Dos señales, ambas de contenido y ninguna de lista fija:

- **Minificado** — media de línea ≥ 400 caracteres **o** una sola línea ≥ 10.000.
  La media sola no bastaba: `index-_Tu1Wy_w.js` promedia 241 caracteres porque
  arrastra miles de líneas de licencias, pero su línea más larga son 77.789.
- **Diccionario** — ≥ 200 líneas, ≥ 90 % de un solo token, media ≤ 40 caracteres.
  Cazó exactamente los seis ficheros de `ZxcvbnData` (`passwords.txt`,
  `english_wikipedia.txt`, `surnames.txt`…), que habrían casado con casi cualquier
  consulta de una palabra.

**Lo que sigue contaminando, cuantificado:** tras los filtros, **el 55,2 % del
índice (2.767 de 5.015 fragmentos) sale de dos familias de directorios**:

| Origen | Fragmentos | % |
|---|---|---|
| `Rockstar Games\Social Club\Launcher\Renderer\…` — cargas de extensiones de Chromium (`listdata.json`, `open_cookie_database.json`, `bindings_main.js`) | 1.737 | 34,6 % |
| `WindowsPowerShell\Modules\` y `PowerShell\Modules\` — módulos de terceros instalados (`Renci.SshNet.xml`, `Posh-SSH.psm1-Help.xml`…) | 1.030 | 20,5 % |

Ninguna señal de contenido los distingue de un documento legítimo: son texto real,
bien formado y en inglés. Lo que los delata es **dónde están**: son el
`node_modules` de PowerShell y de Chromium. Es un problema de exclusión por
directorio, es decir de M1, y **no se ha tocado aquí a propósito** para no
invalidar los números del informe de M1. La recomendación concreta es añadir
`WindowsPowerShell\Modules` y `PowerShell\Modules` a las exclusiones de
`obrain-scan`.

> **Cerrado (2026-08-07)** como paso previo de M4, y con las dos familias, no
> sólo con la que se recomendaba: **2.928 fragmentos retirados, el 55,5 % del
> índice.** La segunda no se pudo resolver con una lista de nombres y obligó a
> preguntarle al directorio. Ver «El saneamiento previo» en el informe de M4.

### Lo que este informe NO cubre

- **`ooxml` no ha corrido nunca contra un archivo real** (ver arriba).
- **PDF escaneado = cero texto, y está bien.** `instructivo.pdf` son 5,4 MB, 30
  páginas, 12 imágenes y **cero fuentes**: es un escaneo. Sale como `empty`, que es
  la respuesta honesta. Hace falta OCR y eso es otro hito con otro coste.
- **La extracción de PDF ensucia con fuentes subconjuntadas.**
  `SanDiskSecureAccessV3.0_QSG.pdf` empieza con ``` ```````````````SIRE ```: glifos
  sin `ToUnicode` utilizable. Un archivo de cinco; no se filtra, se anota.
- ~~**M2 es el hito en el que el contenido sale del archivo, y la sensibilidad (I11)
  todavía no existe.**~~ **Cerrado (2026-08-07)** por
  [`obrain-guard`](../core/obrain-guard) — ver el informe de la puerta más abajo.
  Los dos archivos que este párrafo nombraba, el `.env` con credenciales y
  `VPS password.txt`, son hoy `vault` y están fuera del índice vectorial. El
  diagnóstico de entonces sigue siendo correcto y por eso se conserva: §5.2 permite
  índice **léxico local** incluso para `vault`, así que M2 no violaba nada — pero
  la clasificación tenía que llegar antes que cualquier egreso, y llegó.
- **Sin paralelismo.** 2,68 s para 460 archivos basta; para 200.000 habrá que
  extraer en paralelo y escribir en serie (SQLite tiene un solo escritor, ADR-0002).
- **El chunker es deliberadamente tonto.** `paragraph-pack@1` empaqueta párrafos
  hasta 1.200 bytes con 200 de solape. No entiende tablas, ni encabezados, ni
  código. Lo que importa de él en M2 es que **está versionado** (I7), no que sea
  bueno.

### Informe de la puerta de sensibilidad (2026-08-07)

`obrain-guard` sobre la base que dejan M1, M3 y M2, build release. **Cero llamadas
a un modelo, cero archivos abiertos:** las señales de nombre salen del log y las de
contenido, de la tabla `chunks` que M2 ya había escrito. El clasificador no tiene
acceso al disco, así que no puede hidratar OneDrive ni por descuido.

| | |
|---|---|
| Archivos mirados | 1.314 |
| Con texto que escanear | 454 |
| **`normal`** | **1.304** |
| **`sensitive`** | **3** |
| **`vault`** | **7** |
| | 1.304 + 3 + 7 = 1.314 ✓ |

| Origen de la señal | |
|---|---|
| Del nombre o de la carpeta | 3 |
| Del contenido | 7 |
| | 3 + 7 = 10 clasificados ✓ |

| La puerta | |
|---|---|
| Entidades que M4 podrá vectorizar | 448 de 454 |
| Fragmentos retenidos | 28 de 5.229 (**0,54 %**) |
| Aserciones escritas | 10 |
| Duración | 10,0 s |

**Idempotencia:** segunda pasada → **0 escritas, 0 retractadas, 10 sin cambio.**

**Coste real de la política: medio punto porcentual del índice.** Es el número que
importa. Una política de egreso que dejara fuera el 30 % del corpus sería una que
nadie mantendría encendida.

#### Los diez archivos, uno a uno

Un contador que diga «7 en bóveda» no permite juzgar si sobran o si faltan. Aquí
los dos errores posibles **no son simétricos**: clasificar de más deja un archivo
fuera del índice vectorial y se corrige con un clic; clasificar de menos manda un
pasaporte a un modelo remoto.

| Archivo | Regla | Nivel | ¿Correcto? |
|---|---|---|---|
| `ClipOps\.env` | `nombre:dotenv` | vault | sí |
| `ClipOps\.env.example` | `contenido:variable-con-secreto` | vault | **sí, y sorprende** |
| `ClipOps\.env.production.example` | `contenido:variable-con-secreto` | vault | **sí, y sorprende** |
| `ClipOps\docker-compose.yml` | `contenido:url-con-contrasena` | vault | sí |
| `obrain-guard\src\content.rs` | `contenido:clave-privada-pem` | vault | sí, y es su propio test |
| `…\passwords.txt` | `nombre:secreto` | vault | **no** — es un diccionario |
| `Vps\VPS password.txt` | `nombre:secreto` | vault | sí |
| `MatriculaJuanPabloGarcia.pdf` | `contenido:dni-o-nie` | sensitive | sí |
| `certificado-matricula-…pdf` | `contenido:dni-o-nie` | sensitive | sí |
| `consultaExpte_…pdf` | `contenido:dni-o-nie` | sensitive | sí |

#### El hallazgo del trabajo: el NIE estaba en este repositorio

La primera pasada marcó como `sensitive` dos archivos que no son documentos
personales: **`docs/PROJECT-MASTER.md` y `core/obrain-extract/src/search.rs`**, los
dos por la regla `contenido:dni-o-nie`.

No era un falso positivo. El informe de M2 citaba el nombre real de un expediente
para demostrar que la búsqueda léxica encontraba algo que no estaba en el nombre
del archivo, y ese nombre **llevaba un NIE dentro**. La misma cadena estaba copiada
en un comentario de `search.rs` como ejemplo de consulta con guiones. Las dos
citas están en git, en un repositorio público.

Es exactamente el fallo que el producto existe para evitar, cometido por la
documentación del producto. Y lo destapó la regla más humilde del clasificador: un
identificador no necesita un párrafo alrededor para ser un identificador, le basta
con estar incrustado en el nombre de un archivo.

Queda **redactado en el árbol de trabajo, no en el historial de git**. Rescribir el
historial y decidir si el número debe considerarse ya público es una decisión que
no toma un informe.

> Detalle que merece quedar escrito: la primera redacción sustituyó el NIE por
> `X0000000T`… que también pasa el dígito de control. El clasificador volvió a
> marcar el archivo. Un control de seguridad que caza el intento de arreglarlo es
> la mejor prueba de que no está mirando la lista de nombres.

#### Lo que enseñó auditar las clasificaciones

Tres defectos, ninguno visible en los contadores.

| Defecto | Cómo se vio | Antes | Después |
|---|---|---|---|
| Tres de los siete archivos en bóveda eran `.env.example` | la lista, no el número | 7 vault | regla de plantilla |
| `dni` como subcadena marca `midnight.log` | test escrito al leer la tabla | — | frontera de palabra obligatoria para siglas |
| Una entidad fantasma con contenido caducado | el veredicto no cambiaba al corregir el archivo | — | ver problema abierto |

**El primero se corrigió y el arreglo se volvió contra sí mismo, para bien.** Un
`.env.example` es por convención una plantilla sin valores reales, así que las
reglas de nombre le ceden el paso. Al reejecutar, **dos de los tres siguieron en
bóveda** — pero ya no por su nombre, sino porque la regla de contenido encontró
dentro variables de credenciales que **no son marcadores de posición**. Es la
composición que se buscaba: el nombre cede, el contenido no. Y es un aviso
concreto sobre dos archivos reales de otro proyecto.

**`passwords.txt` es un falso positivo y se deja.** Es el diccionario de
contraseñas comunes de `ZxcvbnData`, no un secreto. Sacarlo del índice vectorial
no cuesta nada —M2 ya lo había descartado como `wordlist`, así que no tiene ni un
fragmento— y la alternativa era peor: hacer que un control de seguridad consultara
una heurística de ruido para decidir. Un control no delega en una heurística.

**`content.rs` en bóveda es correcto y también se deja.** Contiene la cabecera
PEM de una clave privada OpenSSH en un test. La regla ha hecho su trabajo. La
tentación de excluir el propio código fuente es exactamente el tipo de excepción
que convierte una puerta en una puerta con un agujero.

#### Problema abierto que no es de esta capa: entidades fantasma

Al corregir `search.rs` y reescanear, el veredicto no cambió. La causa resultó ser
de M1: **guardar un archivo con un editor que escribe en temporal y renombra le
cambia el `FileID` de NTFS**, así que el escaneo lo ve como un archivo nuevo. Misma
ruta, mismo volumen, dos entidades:

```
7838c90638c8c3fa:b6700000000055010000000000000000   ← antes de editar
7838c90638c8c3fa:e3320200000022000000000000000000   ← después
```

El invariante I8 dice que la identidad `(volumen, FileID) + hash` sobrevive a mover,
renombrar y desconectar discos. Sobrevive a las tres — pero **no al guardado
atómico**, que es como guardan la mayoría de los editores modernos y las
aplicaciones de Office. La entidad vieja se queda con la ruta, con el contenido
caducado y con su clasificación.

Para la puerta el fallo es **hacia el lado seguro**: el fantasma conserva su nivel
antiguo y el archivo nuevo se clasifica de cero. Para M4 no lo es: son dos juegos
de vectores del mismo documento, y la búsqueda semántica devolverá el archivo dos
veces, uno de ellos con texto que ya no existe.

La reconciliación por `(ruta + hash del contenido)` cuando el `FileID` no cuadra es
trabajo de `obrain-scan`, y **no se ha tocado aquí a propósito** para no invalidar
los números de M1.

> **Cerrado (2026-08-07)** como paso previo de M4 — y la regla que se
> implementó **no** exige que el hash coincida, porque este mismo caso
> demuestra por qué no puede exigirlo: al editar `search.rs` cambiaron a la vez
> el `FileID` y el contenido. Ver [ADR-0007](adr/0007-identidad-tras-un-guardado-atomico.md).

#### Lo que este informe NO cubre

- **La bóveda etiqueta, no cifra.** §8.1 pide para `vault` una clave derivada de
  una frase de paso que no se almacena. Eso no existe. Hoy `vault` significa
  «fuera del índice vectorial y fuera de todo egreso», y nada más. La etiqueta
  llega antes que la cerradura.
- **Sólo se ha probado el lado de las negaciones.** No hay ningún proveedor remoto
  conectado todavía, así que `NeedsConsent` y el flujo de consentimiento son
  código sin usar. La puerta niega correctamente; que permita correctamente está
  probado en tests y no contra un integrante real.
- **Las 818 fotos, vídeos y binarios del corpus se clasifican sólo por el
  nombre.** Un pasaporte escaneado como `IMG_2291.jpg` en una carpeta llamada
  `Escritorio` es hoy `normal`. Es el hueco más grande que queda, y no se cierra
  con más reglas de nombre: hace falta OCR, que es otro hito con otro coste.
- **Los 14 documentos de Office deshidratados también.** Mismo motivo que en M2:
  no se abren, así que sólo se les mira el nombre. Son justo los que más se
  parecen a la pregunta de la visa.
- **Un `.ini` o un `.toml` con claves en minúscula se escapa** de la regla de
  variables con secreto, que exige `NOMBRE_EN_MAYUSCULAS=valor` para no marcar
  cada `const token = …` de un `.ts`. Es una frontera elegida, no un descuido.
- **No hay dataset dorado.** §5.1 dice que cada capacidad debe llevar uno. Diez
  clasificaciones auditadas a mano no son una medición de precisión y no se
  presentan como tal.

### Informe de M4 — vectorización y búsqueda semántica (2026-08-07)

`obrain-embed` sobre la base que dejan M1, M3, M2 y la puerta, build release.
Máquina: i5-13420H, 16 GB, sin GPU — la RTX 4050 del equipo **no se usa** y no
hizo falta ([ADR-0008](adr/0008-modelo-de-embeddings-y-runtime.md)).

**174 tests en verde** (145 antes del hito), clippy limpio con `-D warnings`.
Los números de abajo salen todos de **una misma ejecución** del pipeline
completo sobre una base nueva, para que se puedan comprobar entre sí.

#### El saneamiento previo: medir calidad semántica sobre un índice con 55 % de ruido no mide nada

Los dos problemas que M2 y la puerta dejaron abiertos empeoraban justo en este
hito, así que se cerraron **antes** de vectorizar. No es trabajo de M4; es la
condición para que los números de M4 signifiquen algo.

**1 · Las exclusiones.** Dos reglas nuevas en `obrain-scan`, y ninguna es una
lista de nombres que crece para siempre:

| Regla | Cómo decide | Por qué así |
|---|---|---|
| `WindowsPowerShell\Modules`, `PowerShell\Modules` | par **padre/hijo** | `Modules` a secas es una palabra que cualquiera usa para una carpeta suya. `WindowsPowerShell\Scripts` **no** se poda: ahí guarda la gente lo suyo. |
| perfil de Chromium incrustado | **marcador dentro**: `Local State` + `First Run` | Ninguna palabra de `Rockstar Games\Social Club\Launcher\Renderer` dice «esto es un navegador». |

La segunda es la respuesta al problema que M1 dejó abierto —«hace falta un
clasificador de dato de aplicación basado en señales, no una lista que crece»—:
un perfil de Chromium **se identifica a sí mismo**. La comprobación usa
`symlink_metadata` y no `exists`, para que un marcador que fuera un placeholder
de OneDrive tampoco pudiera disparar una hidratación.

Los dos binarios, el anterior y el nuevo, ejecutados sobre **el mismo estado del
disco** para que la comparación sea de verdad una comparación:

| | antes | después |
|---|---|---|
| Archivos vistos | 1.340 | **822** |
| Archivos en el log | 1.333 | **815** |
| Archivos con texto | 471 | **394** |
| **Fragmentos** | **5.435** | **2.507** (−53,9 %) |
| Caracteres de texto | 9.317.586 | 2.914.691 (−68,7 %) |

Lo retirado, por familia:

| Origen | Archivos | Fragmentos | % del índice |
|---|---|---|---|
| Perfil de Chromium (`…\Launcher\Renderer`) | 63 | 1.870 | 34,4 % |
| `WindowsPowerShell\Modules` | 12 | 958 | 17,6 % |
| `PowerShell\Modules` | 2 | 100 | 1,8 % |
| **Total** | **77** | **2.928** | **53,9 %** |

Confirma el 55,2 % que M2 había medido sobre un corpus ligeramente anterior.

Efecto colateral que conviene anotar: `passwords.txt`, el falso positivo que el
informe de la puerta decidió conservar, **ha desaparecido solo**. `ZxcvbnData`
vivía dentro de `WindowsPowerShell\Modules`.

**2 · Las entidades fantasma, medidas con un experimento y no con una consulta.**
Una base recién creada tiene cero por construcción, así que ese número no dice
nada. Lo que mide es reescanear tras editar. Se editó **un** archivo y se
reescaneó con los dos binarios sobre la misma base:

| | código anterior | con reconciliación |
|---|---|---|
| Entidades nuevas | 519 (518 reintroducidas + **1 fantasma**) | **0** |
| Reconciliadas | — | **1** |
| `file_id` retractados | — | 1 |
| **Rutas con más de una entidad viva** | **1** | **0** |

**Una edición = un fantasma, sin techo.** La regla exige tres condiciones a la
vez, y la tercera —que el `FileID` viejo no aparezca en ningún sitio del
recorrido— es la que la hace determinista en vez de heurística. Deliberadamente
**no** exige que el hash coincida: el caso que destapó el problema fue editar
`search.rs`, donde cambiaron a la vez el contenido y el `FileID`. Razonamiento
completo y alternativas en [ADR-0007](adr/0007-identidad-tras-un-guardado-atomico.md).

**3 · Un hallazgo del propio saneamiento: el maestro se fue a la bóveda.** Al
reejecutar la puerta sobre el corpus limpio, `PROJECT-MASTER.md` salió `vault`
por la regla `contenido:clave-privada-pem`. El motivo: el informe de la puerta
explicaba por qué `content.rs` está en la bóveda **citando literalmente la
cabecera PEM**, así que el documento maestro pasó a contener el marcador. Es el
mismo patrón exacto que el NIE — la documentación del control dispara el
control.

No se tocó la regla. Se quitó la cita literal del documento, y al reejecutar la
puerta **retractó** la clasificación: primera vez que se ejercita esa rama, que
en el informe anterior aparecía como «0 retractadas». El maestro queda
`sensitive` porque el identificador de la redacción anterior sigue escrito aquí,
y §5.2 concede a `sensitive` índice vectorial local, así que sigue dentro.

#### El corpus con el que se midió

| | |
|---|---|
| Archivos en el log | 815 |
| Con texto extraído | 394 |
| Fragmentos | 2.507 |
| **Entidades que la puerta concede** | **388** |
| Entidades retenidas (`vault`) | 6 |
| **Fragmentos retenidos** | **28** (1,12 %) |

Los 28 se desglosan exactamente: `.env` 3 + `.env.example` 2 +
`.env.production.example` 2 + `docker-compose.yml` 2 + `content.rs` 18 +
`VPS password.txt` 1 = **28** ✓

#### Vectorización

| | |
|---|---|
| Modelo | `multilingual-e5-small@1`, ONNX fp32, 384 dimensiones |
| Carga del modelo | 2,50 s |
| Fragmentos pendientes | 2.479 |
| **Vectores escritos** | **2.479** |
| Dentro del modelo | 275,91 s |
| Total | 277,51 s (**4,6 min**) |
| Ritmo | 9 fragmentos/s |
| Índice vectorial | **3,63 MB** |
| Base de datos completa | 14,55 MB |
| Aserciones escritas | 776 (2 por entidad vectorizada) |

**Aritmética verificada** — el CLI la imprime y marca si no cuadra:

```
388 concedidas + 6 retenidas = 394 entidades con texto   ✓
2.507 fragmentos - 28 retenidos = 2.479 vectorizables    ✓
vectores en el indice = 2.479                            ✓
rechazos en la comprobacion por invocacion = 0           ✓
```

Ese último cero es el que importa. La puerta se consulta **dos veces**:
`gate::embeddable` decide el conjunto de partida y `gate::check` se llama por
entidad justo antes de invocar el modelo. `denied_at_call = 0` significa que las
dos están de acuerdo; el día que no lo estén, lo dirá un número en vez de pasar
inadvertido.

**Idempotencia:** segunda pasada → **0 pendientes, 0 vectores, 0 aserciones
escritas, 776 sin cambio.**

#### El criterio de "hecho": encontrar por significado, no por nombre

La prueba tiene que ser una consulta cuyas palabras **no estén** en el documento
objetivo, y la comparación honesta contra BM25 para esa misma consulta.

**Consulta: `universidad`.**

```
BM25 →  1. -7,66  core/obrain-extract/src/format/ooxml.rs
        2. -7,52  docs/adr/0011-rerank-diferido.md
        3. -6,84  docs/PROJECT-MASTER.md
        (ni en el puesto 50 aparece el documento correcto)

kNN  →  1. 0,850  Escritorio\MatriculaJuanPabloGarcia.pdf     ← no contiene la palabra
        2. 0,848  Orchestrator's-Brain\README.md
        3. 0,848  ClipOps/src/app/clips/[id]/clip-analysis.tsx
```

El documento correcto es un formulario de matrícula que dice «Titulación»,
«Centro Docente Privado», «Año académico» y «Asignaturas», y **no dice
"universidad" en ningún sitio**. BM25 no puede encontrarlo por definición: los
sitios donde esa palabra está escrita son un fixture de test y —efecto
observador— los documentos de este mismo hito, que se escribieron dentro del
corpus que estaban midiendo. El índice vectorial lo pone primero.

Es la primera vez que el sistema encuentra algo por lo que significa y no por lo
que dice. Y la comparación es honesta en los dos sentidos: **la fusión RRF lo
deja fuera del podio**, porque BM25 está muy seguro de sus aciertos literales y
varios de ellos aparecen también en la rama vectorial. Es exactamente el trabajo
que hace un rerank, y el rerank está diferido — ver abajo.

#### El otro hallazgo: la primera relación semántica que ningún enlazador determinista podía ver

El vecino más cercano de `MatriculaJuanPabloGarcia.pdf` es
`certificado-matricula-JuanPabloGarciaMonsalve.pdf`, con **0,927**. Son dos
documentos del mismo trámite, con nombres distintos, hashes distintos y sin una
sola arista directa entre ellos: M3 sólo los relacionó por `contained_by` con la
carpeta que comparten.

Y no se materializa como arista. **I3 lo prohíbe** —la similitud semántica es
una operación de consulta, no una arista— precisamente para que el grafo no se
convierta en niebla. Aquí se ve por qué la regla es correcta y por qué duele:
la relación es real y valiosa, y aun así se calcula cada vez.

#### Lo que enseñó auditar el espacio vectorial

Tres cosas, y **ninguna aparece en ningún contador**. La cobertura es del 100 %,
la aritmética cierra, y las tres estaban ahí.

**1 · Los hubs existen y tienen nombre y apellidos.** Si el espacio fuera sano,
cada fragmento saldría unas 10 veces entre los 10 vecinos más cercanos de los
demás. El peor sale **97 veces, casi 10× lo esperado**:

| veces | × esperado | fragmento |
|---|---|---|
| 97 | 10× | `Martian-Roulette\package-lock.json` |
| 82 | 8× | `Cheddar-Chase\package-lock.json` |
| 79 | 8× | `losFrutosDeMaria\backend\package-lock.json` |
| 78 | 8× | `My Games\Rocket League\TAGame\Config\TAInput.ini` |

Los diez peores son **sólo dos cosas**: archivos de bloqueo de npm y la
configuración de mandos de un juego. Y en el otro extremo, **300 fragmentos
(12,1 %) no son vecinos de nadie**.

Es la misma patología que M2 destapó con «quién ocupa el índice», un piso más
abajo: texto escrito por una máquina para una máquina, que sobrevivió a
`noise.rs` porque no está minificado ni es un diccionario. Cuantificado:

| Familia | Archivos | Fragmentos | % del índice |
|---|---|---|---|
| `package-lock.json` | 4 | 165 | 6,6 % |
| Logs del lanzador de Rockstar | 17 | 118 | 4,7 % |
| `My Games\…\*.ini` (configuración de juegos) | 4 | 82 | 3,3 % |
| Otros archivos de bloqueo | 1 | 55 | 2,2 % |
| **Total** | **26** | **420** | **16,8 %** |

**No se ha tocado**, por el mismo motivo por el que M2 no tocó las exclusiones:
es trabajo del clasificador de dato de aplicación, y es la recomendación
concreta que este informe deja escrita, igual que M2 dejó la suya.

**2 · El coseno no es un umbral de relevancia, sólo un orden.** Los vecinos de
`MatriculaJuanPabloGarcia.pdf` incluyen un `main.js` a **0,885**, por delante de
documentos relacionados de verdad. En este corpus casi todo cae entre 0,82 y
0,94: la banda entera son 0,12 puntos. Un umbral absoluto de similitud —«mostrar
lo que pase de 0,85»— sería ruido puro.

Esto **valida RRF por una vía distinta a la prevista**. §6 elegía fusión por
rangos porque las escalas de BM25 y del coseno no son comparables; la medición
añade que el propio coseno apenas es comparable consigo mismo. Sólo el orden
sobrevive.

**3 · Dónde se rompe, con ejemplos y con el puesto exacto.** Un informe que diga
«la búsqueda semántica funciona» sin enseñar una consulta que falla es una nota
de prensa:

| Consulta | Documento correcto | Puesto en kNN | BM25 |
|---|---|---|---|
| `papeles de la universidad` | `MatriculaJuanPabloGarcia.pdf` | **1** (0,847) | no lo devuelve ni en el 50 |
| `en que ano naci` | el mismo (dice «Fecha nacimiento: 01/12/2004») | **6** (0,833) | no lo devuelve |
| `donde vivo` | el mismo (dice «Dirección: CL HÉROE DE SOSTOA…») | **37** (0,823) | no lo devuelve |
| `contrasena de acceso a la maquina remota` | `Vps\VPS password.txt` | **fuera** — está en la bóveda, y es correcto | — |

El diagnóstico está en los números: entre el puesto 1 y el 37 hay **0,04 puntos
de coseno**. Las preguntas cortas y genéricas no separan nada, y un puñado de
hubs a 0,85 se cuelan por delante de la respuesta correcta a 0,82. Es el mismo
16,8 % de basura del punto 1 haciendo su trabajo.

El último caso merece leerse dos veces: la respuesta correcta existe, el sistema
la tiene indexada léxicamente, y **la puerta la deja fuera del índice vectorial a
propósito**. Funciona como debe. Es la primera vez que se ve el coste de la
política sobre una consulta concreta en vez de sobre un porcentaje.

#### El defecto que sólo apareció ejecutando el pipeline entero

Los 173 tests estaban en verde, clippy limpio, la aritmética cerrada y el corpus
vectorizado. Entonces se editaron cinco archivos del repositorio y se volvió a
pasar el pipeline completo:

```
▸ 815 archivos en el log
error al extraer: sqlite: FOREIGN KEY constraint failed
```

`obrain-extract` **borra y reinserta** los fragmentos de un archivo que ha
cambiado. La tabla `vectors` los referenciaba sin `ON DELETE CASCADE`, así que
el borrado chocaba contra la clave ajena. Efecto: **cualquiera que editara un
archivo ya vectorizado se quedaba sin poder reextraer nada.**

Ningún test podía verlo. Los de M4 no borran fragmentos; los de M2 no conocen la
tabla `vectors`, que se creó dos hitos después. Es la forma que toma un defecto
cuando una proyección referencia a otra: **añadir una capa rompe la de abajo sin
tocarla**, y la única prueba que lo detecta es ejecutar la cadena entera contra
datos que cambian.

El arreglo es `ON DELETE CASCADE`, que además es la semántica correcta —si el
texto cambió, el vector de ese texto es basura y la cola de pendientes lo vuelve
a recoger— y un test de regresión con el caso real. Poner al extractor a borrar
vectores habría sido la otra salida, y habría obligado a M2 a conocer M4.

#### El ensayo del test canónico

> *«el documento que usé para demostrar ingresos durante mi trámite de visa»*

```
RRF →  1. docs/PROJECT-MASTER.md       [bm25#1 knn#2]
       2. README.md                    [bm25#3 knn#1]
          «...Puedes buscar por significado: "el documento que usé
           para demostrar ingresos" es una búsqueda válida...»
```

El sistema devuelve **la documentación del propio proyecto**: encontró la frase
que *describe* la pregunta, no el documento que la *responde*.

Y es la respuesta correcta, porque en este disco no hay nada que responderla. Los
14 documentos de Office —presupuestos, actas, revisiones— siguen deshidratados en
OneDrive y sin texto extraído, y son justo el material que se parece a la
pregunta. No cuenta como fallo de M4, tal y como estaba previsto.

De la frase, M4 resuelve la mitad semántica —«documento», «demostrar
ingresos»— y **no toca** «durante mi trámite de visa», que es resolución de
entidad y de rango temporal, es decir M7.

#### El presupuesto de latencia, medido

| | | techo a 50 ms |
|---|---|---|
| Consulta híbrida completa (BM25 + kNN + RRF), incluido embeber la consulta | **43 ms** | |
| kNN lineal sobre 2.479 vectores, leyendo de SQLite | 6,3 ms | ~19.700 vectores |
| kNN lineal con la matriz ya en memoria | **0,57 ms** | ~219.000 vectores |

**El 91 % del coste del kNN es deserializar, no multiplicar.** Es el hallazgo que
decide la arquitectura del índice, porque invierte la conclusión: sin separar las
dos cifras, el techo del recorrido lineal parece estar en ~19.700 vectores —unos
3.000 archivos, que este corpus alcanza en cuanto crezca— y lo natural sería
concluir «hará falta un índice aproximado pronto». Con las dos cifras, lo que
hace falta cuando eso ocurra no es aproximar sino **dejar de deserializar**: la
matriz residente cuesta 3,6 MB hoy y sube el techo a ~219.000 vectores sin
renunciar a la exactitud. `sqlite-vec` no entra hasta ahí. Umbrales escritos en
[ADR-0010](adr/0010-recorrido-lineal-frente-a-indice-ann.md).

**El rerank se difiere, con la medición delante.** No se ha medido
`bge-reranker-v2-m3` —son 2,2 GB sin descargar—, pero sí el **suelo**: 50
secuencias largas por el modelo de 118 M que ya está cargado son **4,5 s**. Un
cross-encoder de 568 M sobre el mismo top-50 no puede ser más rápido. La
extrapolación por parámetros da ~21 s y está etiquetada como extrapolación, no
como medición; el suelo de 4,5 s ya basta por sí solo frente a los 43 ms de la
consulta entera y a los 0,5–1,9 ms que M2 fijó como «instantánea». Razonamiento y
alternativas en [ADR-0011](adr/0011-rerank-diferido.md).

#### La puerta, probada contra este pipeline y no contra sí misma

`obrain-guard` ya tenía `embeddable_excluye_la_boveda`, que demuestra que la
consulta de origen deja fuera la bóveda. Eso demuestra que la puerta funciona;
**no demuestra que M4 la use**. Un `SELECT DISTINCT entity FROM chunks` en
`embed_all` habría pasado aquel test y filtrado igual.

Los cinco tests de `obrain-embed/tests/puerta.rs` atacan el pipeline:

- una entidad `vault` con fragmentos **no acaba con un solo vector**;
- su texto **ni siquiera se le ofrece al modelo** — el embebedor de prueba
  registra todo lo que recibe y se comprueba que el secreto no está;
- tampoco deja procedencia en el log;
- reclasificar a bóveda y **reconstruir** la proyección la saca del índice;
- lo `sensitive` **sí** se vectoriza en local, que es lo que dice §5.2 y lo que
  evita que la política sea de las que se acaban apagando enteras.

#### Lo que este informe NO cubre

- **No hay dataset dorado.** §5.1 dice que cada capacidad debe llevar uno.
  Cuatro consultas auditadas a mano no son una medida de precisión y no se
  presentan como tal. Es la carencia más grande de este informe: sin un conjunto
  de pares (consulta, documento esperado), «la calidad mejoró» seguirá siendo
  una opinión — y sin él, la migración a `bge-m3` de [ADR-0008](adr/0008-modelo-de-embeddings-y-runtime.md)
  no se podrá justificar con datos.
- **El rerank no se ha medido, se ha acotado por abajo.** Ver arriba.
- **`candle` no se ha comparado con `ort`.** ADR-0008 lo descarta por madurez,
  no por una medición. Implementar las dos rutas era el hito entero.
- **El «binario nativo único» de §7.2 ya no es exacto.** ONNX Runtime se enlaza
  estáticamente, pero la distribución de Windows arrastra `DirectML.dll` junto
  al ejecutable; Windows 10 1903+ la trae en `System32`, así que funciona, pero
  es una dependencia externa nueva. Y `ort` descarga su librería nativa **en
  tiempo de compilación** desde una CDN de terceros.
- **La descarga del modelo no verifica firmas ni hashes.** Se apoya en TLS y en
  el repositorio oficial, que es lo mismo que cualquier `pip install` y no es
  suficiente.
- **No hay presupuesto ni reanudación.** `embed_all` vectoriza todo lo pendiente
  dentro de una sola transacción. Con 2.479 fragmentos son 4,6 minutos; con
  200.000 sería una transacción de horas. §4.4 pide una cola diferida priorizada
  por atención del usuario, y eso no existe.
- **El chunker sigue siendo el de M2, y ahora importa más.** `paragraph-pack@1`
  empaqueta párrafos hasta 1.200 bytes sin entender tablas, encabezados ni
  código. En BM25 eso reparte términos; en un espacio vectorial produce
  fragmentos cuyo *significado medio* no es el de nada en concreto — que es una
  causa candidata de los hubs, y no se ha comprobado.
- **El 17 % de fragmentos escritos por una máquina sigue dentro** (tabla arriba).
  Es la recomendación concreta para el siguiente hito, igual que M2 dejó la suya.
- **Reclasificar a bóveda no borra los vectores existentes.** Sólo los quita al
  reconstruir la proyección. Hay un test que fija ese comportamiento, y una
  ventana en la que un documento es `vault` y sus vectores siguen ahí.
- **Sin paralelismo y sin GPU.** 9 fragmentos/s con un solo hilo de control. La
  RTX 4050 del equipo no se ha tocado y no hizo falta; para 200.000 fragmentos
  esa frase habrá que revisarla.
- **Las otras dos ramas de §6 no existen.** Travesía de grafo desde entidades
  resueltas y filtro temporal bitemporal. La forma del pipeline las admite —son
  dos `Ranking` más en la llamada a `rrf`— pero necesitan resolución de entidad,
  que es M7.

---

## 10. Decisiones pendientes

- ¿El conocimiento sobrevive al archivo que lo originó cuando ese archivo desaparece? (Postura provisional: sí — el nodo es *conocimiento sobre* el archivo, no el archivo.)
- ¿Habrá alguna vez conocimiento compartido entre personas? No implementarlo ahora es correcto; ignorarlo en el modelo de datos, no.
- ¿El sistema escribe alguna vez en los archivos del usuario? (Postura provisional: v1 estrictamente solo-lectura; el derecho a escribir se gana.)
- Slug técnico: `orchestrator-brain` u `obrain`. El apóstrofo de `Orchestrator's-Brain` dará guerra en scripts, CI, rutas y URLs.

---

## Registro de decisiones

| ADR | Título | Estado |
|-----|--------|--------|
| [0001](adr/0001-log-de-aserciones-como-fuente-de-verdad.md) | Log de aserciones como fuente de verdad | Aceptada |
| [0002](adr/0002-sustrato-de-persistencia.md) | SQLite como sustrato único de persistencia | Aceptada |
| [0003](adr/0003-lenguaje-del-nucleo.md) | Rust para el núcleo de indexación | Aceptada |
| [0004](adr/0004-punto-de-entrada-barra-no-galaxia.md) | La barra de comandos es el punto de entrada; el mapa es el sustrato | Aceptada |
| [0005](adr/0005-donde-vive-el-texto-extraido.md) | El texto extraído es una proyección, no una aserción | Aceptada |
| [0006](adr/0006-donde-vive-la-sensibilidad.md) | La sensibilidad es una columna impuesta con procedencia en el log | Aceptada |
| [0007](adr/0007-identidad-tras-un-guardado-atomico.md) | La identidad se reconcilia por ruta cuando el `FileID` se rompe | Aceptada |
| [0008](adr/0008-modelo-de-embeddings-y-runtime.md) | `multilingual-e5-small` sobre ONNX Runtime | Aceptada |
| [0009](adr/0009-donde-viven-los-vectores.md) | Los vectores son una proyección y su identidad I7 está en la clave primaria | Aceptada |
| [0010](adr/0010-recorrido-lineal-frente-a-indice-ann.md) | Recorrido lineal en vez de `sqlite-vec`, y el umbral donde deja de valer | Aceptada |
| [0011](adr/0011-rerank-diferido.md) | El rerank se difiere, con la medición que lo justifica | Aceptada |

## Documentos

| Documento | Contenido |
|-----------|-----------|
| [EXPERIENCE.md](EXPERIENCE.md) | Fase 3 — lenguaje visual, animaciones, navegación, arranque en frío, evolución del mapa |
