# Orchestrator's Brain — Documento Maestro

> Las aplicaciones actuales almacenan información. Orchestrator's Brain construye conocimiento.

**Estado:** Fase 2 — Arquitectura · **Última revisión:** 2026-08-10 · **Documento vivo**

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
| ✅ **M5a — La barra sobre el sustrato** | Superficie del núcleo, lente de I2, barra de ADR-0004. → [`core/obrain-api`](../core/obrain-api) + [`ui`](../ui) | **Hecho (2026-08-08).** Ver informe abajo. |
| **M5b — El mapa de frente** | Tecla global, modos foco y deriva, panel de entidad, expansión, Trazo, y el conmutador galaxia/explorador de [ADR-0014](adr/0014-dos-vistas-un-solo-estado.md). | Se puede explorar un nodo concentrador y sus artefactos visualmente. |
| **M5c — Enterarse** | Vigilancia continua: USN Change Journal para el volumen, `ReadDirectoryChangesW` para carpetas concretas (§4.1). El demonio de [ADR-0003](adr/0003-lenguaje-del-nucleo.md). | Se deja un archivo nuevo en una carpeta vigilada y **aparece en el mapa sin que nadie lance nada**. |
| **M6 — Construir conocimiento** | Extracción de entidades, detección de `Process`, enriquecimiento por atención. | La métrica de §0 es medible y positiva. |
| **M7 — La pregunta** | Comprensión de consulta, resolución temporal, respuesta con citas. | **Responde la pregunta de la visa.** |

M3 es el hito más subestimado y probablemente el que más valor por hora de trabajo entrega. M7 es el que valida la tesis.

### Por qué M5 se partió en dos, y por qué su criterio ya no dice `Process`

Las dos cosas se decidieron **antes** de escribir código, en 2026-08-08, y conviene que quede el porqué y no sólo el resultado.

**El criterio anterior era inalcanzable por construcción.** Decía *«se puede explorar un `Process` y sus artefactos visualmente»*, y `Process` no existe: lo crea M6. Un hito cuyo criterio de hecho depende del entregable del hito siguiente no se puede cerrar nunca. La salida no es esperar a M6, porque lo que M5 tiene que demostrar —que la lente acota, que el layout es estable, que la barra responde— no necesita un `Process` para demostrarse: necesita **un nodo y sus artefactos**, y de eso hay 353 desde M3. Las carpetas y las sesiones son estructuralmente la vista de la visa sin el nombre. El modo Proceso de [`EXPERIENCE.md`](EXPERIENCE.md) §4.3 queda como forma preparada y sin datos, que es lo que es.

**Y el alcance estaba subestimado.** «Lente, layout estable, navegación por foco» suena a una vista de grafo, pero M5 es el hito donde aparece la aplicación entera: el primer Tauri, el primer TypeScript, el primer PixiJS, el primer contrato contra el núcleo — y la barra, que [ADR-0004](adr/0004-punto-de-entrada-barra-no-galaxia.md) declara punto de entrada. Construir el mapa sin la barra sería construir exactamente lo que ese ADR rechaza, y además sin puerta por donde entrar. De ahí el corte: **M5a es el camino de las cuarenta veces al día**, M5b es el mapa de frente.

**El orden frente a M6 no es preferencia, es dependencia.** §4.4 pide que el enriquecimiento vaya en una cola *«priorizada por atención del usuario: abierto recientemente, dentro de un foco activo, tocado por una búsqueda»*. Esas señales no existen sin interfaz. M6 antes que M5 obligaría a inventar una política de priorización sin la fuente de datos que la política necesita.

### Por qué M5c existe, y por qué va justo detrás de M5b

La vigilancia continua estaba **diseñada desde §4.1 y programada en ninguna parte**: tecnología elegida, citada en dos ADRs, y sin ser el criterio de «hecho» de ningún hito. Así es como una pieza se cae de un proyecto — no por olvidada, por no programada. El usuario lo señaló el 2026-08-10 con la frase que lo resume: *las cosas van a seguir llegando a la máquina aunque el programa ya esté terminado.*

**Y tiene una dependencia real que la obliga a existir, no sólo una justificación de producto.** [`EXPERIENCE.md`](EXPERIENCE.md) §3 define seis animaciones, y la sexta —**Sedimentación**— dice: *«El conocimiento nuevo creado en segundo plano aparece con un fundido lento en la periferia»*. Sin vigilancia continua no hay segundo plano, así que esa animación no tiene fuente de datos: M5b puede dibujarla, pero no puede demostrarla. M5c es lo que convierte la sexta animación en algo que se puede ver ocurrir en vez de simular.

Va **detrás** de M5b y no delante porque hasta que el mapa no esté de frente no hay dónde ver aparecer nada, y porque el coste de esperar es bajo: reescanear el perfil entero cuesta 0,60 s, así que mientras tanto un temporizador cubre el hueco sin deuda arquitectónica.

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

### Informe de M5a — la barra sobre el sustrato (2026-08-08)

[`core/obrain-api`](../core/obrain-api) y [`ui`](../ui), sobre una base nueva
construida escaneando **este mismo repositorio**. **201 tests en verde** (174
antes del hito), clippy limpio con `-D warnings`, TypeScript limpio en modo
estricto.

Corpus pequeño a propósito: lo que M5a tiene que demostrar no es escala —eso ya
lo midieron M1 a M4— sino que la lente acota, que el layout no baila y que la
barra responde.

| El corpus | |
|---|---|
| Archivos | 101 |
| Aristas vivas | 234 — `contained_by` 133, `occurred_during` 100, `duplicate_of` 1 |
| Nodos concentradores | 38 |
| Fragmentos | 1.042 |
| Puerta | 99 `normal` · 1 `sensitive` · 1 `vault` |

**El criterio de "hecho", medido.** Consulta `sensibilidad`, sin modelo cargado:
10 resultados en **10,5 ms** por la rama léxica, y el sustrato que abren son
**21 nodos y 24 aristas de 116 alcanzados** — es decir, la lente descartó el
82 % del vecindario y lo dijo. De esas 24 aristas, 14 salen sólidas y 10
punteadas, y un nodo llega marcado por la puerta. Los cuatro canales visuales de
[`EXPERIENCE.md`](EXPERIENCE.md) §1 viajan resueltos desde el núcleo
([ADR-0012](adr/0012-el-nucleo-expone-una-superficie.md)).

#### El hallazgo del hito: el cuello de botella no es el render, es el layout

El prototipo [`01-graph-render`](../prototypes/01-graph-render/index.html) midió
**0,6 ms** para 500 nodos y concluyó, con razón, que *«el render no es el cuello
de botella, ni de lejos»*. M5a mide la otra mitad del fotograma —colocar los
nodos— sobre WebView2, mediana de cinco pasadas en caliente:

| Nodos | Redibujado completo |
|---|---|
| 50 | 16,3 ms |
| **120** | **40,2 ms** |
| 200 | 102 ms |
| **300** (presupuesto de I2) | **196,7 ms** |

Es cuadrático porque la repulsión es todos contra todos: a 300 nodos son **19,8
millones** de comparaciones por redibujado. Y el sustrato se redibuja **en cada
pulsación**, así que 197 ms no es un detalle de rendimiento: es la diferencia
entre la barra de §2 y una que se arrastra.

La consecuencia es una constante nueva, `SUBSTRATE_BUDGET = 120`, y la
justificación tiene dos patas que apuntan al mismo sitio. La de diseño llegó
primero: **el sustrato no es el modo Foco.** Lo que se ve de reojo tiene que
decir *dónde* cae la respuesta, y para eso trescientos puntos son ruido. La
medida lo confirmó. El presupuesto de 300 se conserva donde sí corresponde —al
enfocar con Enter, una vez por acción explícita y no por tecla.

**Lo que esto NO resuelve, y está escrito en §7.4 desde antes:** *«simulación de
fuerzas en un worker»*. Sacar el layout del hilo principal es lo que permitiría
volver a 300 nodos en el sustrato sin bloquear nada. Es la recomendación
concreta que este hito deja a M5b, igual que M2 y M4 dejaron la suya.

#### Lo que enseñó dibujar el grafo por primera vez

Cuatro defectos. **Ninguno lo veía un test, y tres llevaban meses ahí.**

| Defecto | Cómo se vio | Desde |
|---|---|---|
| Los nodos de sesión no tenían nombre | los tres más grandes del mapa se llamaban `<01KZHYVT>` | **M3** |
| Cinco carpetas distintas llamadas `src` | el censo previo, antes de dibujar nada | M3 |
| Identificadores con prefijo común caían encima | generar el icono de la aplicación | M5a |
| La página no cargaba su propio módulo | el canvas seguía a 300×150 | M5a |

**El primero es el que merece leerse.** `session_span` guarda el intervalo de
una sesión en el **eje de validez** —la sesión ocurrió entonces, no ahora—, que
es exactamente lo que I6 pide. Consecuencia: `resolve` con el presente **nunca**
lo encuentra, porque `valid_to` ya pasó. No es un fallo de la bitemporalidad:
es la bitemporalidad funcionando, chocando contra una pregunta que nadie había
hecho todavía. Cuando el intervalo **es el dato** en vez del filtro, hace falta
la otra consulta.

De ahí sale `Store::resolve_whenever` — *qué sostiene el sistema, fuera cuando
fuera cierto*— y no es una puerta trasera: lo retractado sigue sin aparecer. Lo
importante es que **es la primitiva que necesita M7**: resolver *«durante mi
trámite de visa»* exige leer el intervalo de un `Process` sin conocerlo de
antemano, que es literalmente esta consulta. M5a la ha pagado por accidente,
persiguiendo una etiqueta.

**El tercero lo destapó un icono.** El generador dibujaba once nodos y salían
dos. FNV-1a sin paso de remate apenas propaga los últimos bytes de la entrada:
dos cadenas que difieren en el último carácter salen con los veinte bits altos
idénticos. Importa porque **los ULID comparten prefijo por construcción** —los
primeros diez caracteres son la marca de tiempo— y toda la diferencia entre dos
entidades del mismo escaneo vive al final. Sin el finalizador de `splitmix64`,
cuarenta entidades de una misma pasada arrancarían todas en el mismo punto.

**El cuarto no es interesante y por eso se cuenta.** Faltaba el `<script>` en
el HTML. Ni un error en consola, ni un test en rojo: la página cargaba, el CSS
se aplicaba y no pasaba nada. Lo delató mirar el tamaño del canvas.

#### El apóstrofo, que ya no es una decisión pendiente teórica

§10 lleva diciendo que `Orchestrator's-Brain` *«dará guerra en scripts, CI,
rutas y URLs»*. La primera factura llegó en el primer hito con cadena de
herramientas de JavaScript:

```text
resource.rc(26) : error RC2135 : file not found:
  C:\...\GitHub\Orchestrator\'s-Brain\ui\src-tauri\icons\icon.ico
```

`tauri-winres` escribe la ruta del icono pasada por su `escape_string`, que
convierte `'` en `\'`; `RC.EXE` sólo entiende `\\` y `\"`, así que busca una
carpeta llamada literalmente `Orchestrator\'s-Brain`. **No se arregla con un
enlace de directorio**: `tauri-build` canonicaliza la ruta y vuelve a salir la
real — comprobado.

`ui/src-tauri/build.rs` lleva un rodeo: copia el icono a una ruta temporal sin
comillas. Está **condicionado a que haya apóstrofo**, así que el día que la
carpeta se renombre el parche desaparece solo. Lo que no arregla es la clase de
problema: `npm` aguanta la ruta, `vite` la aguanta, `RC.EXE` no. El siguiente
que no la aguante costará otro rodeo.

### Corrección de M5a — el mapa pasa a ser la pantalla (2026-08-09)

M5a se entregó siguiendo [ADR-0004](adr/0004-punto-de-entrada-barra-no-galaxia.md):
barra delante, mapa de fondo. Con la aplicación funcionando el veredicto fue que
no, y **la causa resultó no ser la disposición sino una carencia de datos**. Se
corrigió el mismo día; el razonamiento completo está en
[ADR-0013](adr/0013-el-mapa-es-la-pantalla.md), que sustituye al 0004.

Dos carencias, las dos del núcleo y ninguna de pintura:

**1 · La búsqueda devolvía archivos planos.** El sistema ya sabía agruparlos:
M3 lleva desde el tercer hito creando concentradores y la superficie los
ignoraba. `sky::cluster` los agrupa por la carpeta que comparten. En el corpus
de prueba, `sensibilidad` pasa de cuatro filas a **dos sitios**.

**2 · No existía con qué dibujar un cielo.** La única lente era foco + radio
(I2), que responde *«esto y lo que tiene al lado»*. `sky::regions` añade la otra
escala —la «agregación semántica por escala» que
[`EXPERIENCE.md`](EXPERIENCE.md) §6 exigía diseñar *ahora*— partiendo el árbol
de contención por la región mayor, repetidamente, como un zoom.

| Sobre el corpus del repositorio | |
|---|---|
| Archivos | 101 |
| **Regiones abiertas** | **12** · `ui` 21 · `docs` 16 · `obrain-extract` 14 … |
| Cobertura | **101 / 101** — ningún archivo fuera del mapa |
| Puentes dibujados | 18 de 41 encontrados |
| Coste | **3,1 ms** |

**Los puentes son el hallazgo de la corrección.** Una sesión de trabajo que tocó
archivos de dos regiones no es un nodo: es una **arista entre esas dos
regiones**, y sale de datos que M3 escribía desde hace tres hitos. Es
exactamente la forma que §6 llama «Puentes» en el mes 1–3. La más fuerte de este
disco une `obrain-core` con `docs`, seis sesiones — que es literalmente lo que
pasó hoy.

Y hubo que ponerles presupuesto: **41 puentes para 66 parejas posibles** es un
ovillo que dice «todo se relaciona con todo», que es lo mismo que no decir nada.
Se enseñan los más fuertes y se dice cuántos había, igual que hace la lente con
los nodos.

#### Dos defectos más que sólo aparecieron al agregar

| Defecto | Cómo se vio | Desde |
|---|---|---|
| El árbol de carpetas estaba **partido en trozos** | regiones a profundidad 0, como si fueran raíces | **M3** |
| Treinta regiones sobre cien archivos es ruido | pedir 30 y obtener 30 etiquetas sobre carpetas de tres archivos | M5a |

**El primero llevaba desde M3.** `structure.rs` sólo encadenaba **un nivel**
hacia arriba desde cada carpeta que contuviera archivos, así que una carpeta
intermedia que sólo tiene subcarpetas —`prototypes`, `src`, un año dentro de un
álbum de fotos— nunca recibía su arista y el grafo quedaba en pedazos. No lo
veía ningún contador: las aristas salían, sólo que el árbol no era un árbol.
Encadenar hasta la raíz del volumen añadió **6 aristas y 7 nodos** en este
corpus y unió los trozos.

**El segundo es una calibración.** El objetivo de regiones pasa a ser un
**techo, no una cuota**: se abren `min(⌈√archivos⌉, objetivo)`. Cien archivos
→ 10 regiones; diez mil → 30. Y al partir no se promueven todas las hijas de
golpe —una carpeta de fotos con doscientos años dentro daría doscientas
estrellas—: suben las mayores y el resto queda agregado bajo la madre.

### El pase visual (2026-08-09)

Con la estructura puesta, el mapa se pintó de verdad. La regla no se relajó ni
una vez: **§1 no admite un canal visual que no codifique algo**, así que cada
efecto tuvo que ganarse su dato antes de existir.

| Lo que se ve | Lo que significa | De dónde sale |
|---|---|---|
| Tamaño de la estrella | archivos dentro | `Region.weight` |
| **Temperatura** | cuándo tocaste algo ahí por última vez | `Region.recency` ← **nuevo** |
| **Polvo** | un punto por archivo | la densidad *es* la cantidad |
| **Latido** | recencia — lo vivo respira | `recency`, §3 #4 |
| Luminosidad | relevancia frente a la consulta | `encendidas` |
| Profundidad | — | parallax por capas (§7.5) |

**La temperatura es la que cumple la promesa del README.** *«Las zonas
brillantes son donde tu vida está activa ahora mismo»* dejó de ser una frase:
`recency` viaja dentro de cada región y de cada nodo, y se mapea a un degradado
frío→cálido **con una curva y no una recta**, porque entre *ayer* y *anteayer*
hay una diferencia enorme y entre *hace tres años* y *hace cuatro* casi ninguna.
En este disco, `ui` y `docs` salen al 100 % de calor y `obrain-link` a 0: es
exactamente lo que se ha tocado hoy y lo que no.

**El polvo es cartografía, no textura.** Un punto por archivo, disperso de forma
determinista alrededor de su región. La densidad codifica la cantidad igual que
el tamaño — es un mapa de puntos de toda la vida, no relleno decorativo.

#### Lo medido

Sobre WebView2, mediana de cuarenta pasadas:

| | fotograma p50 | p95 |
|---|---|---|
| 12 regiones · 125 sprites | **0,10 ms** | 1,0 ms |
| 30 regiones · 3.060 sprites | **0,40 ms** | 1,1 ms |

Contra los 16,67 ms de 60 fps. **El latido sale gratis** — y no por suerte: los
astros son `Sprite` con una textura de gradiente compartida, así que se agrupan
en una sola llamada de dibujo, mientras que la versión anterior construía la
geometría de tres círculos por nodo con `Graphics`. Se ve mucho mejor y cuesta
menos, que es la clase de cambio que sólo aparece cuando se mide antes.

Confirma por tercera vez lo que dijo el prototipo `01-graph-render` y volvió a
decir la corrección de M5a: **el render no es el cuello de botella.** Lo es el
layout, que sigue siendo O(n²) en el hilo principal.

#### El reposo, que sigue siendo un compromiso

§2 pide que el bucle caiga a cero con la ventana sin foco o el usuario inactivo.
Se cumple literal: el ticker arranca al haber actividad, se para solo a los 25 s
y con `prefers-reduced-motion` no arranca nunca. Y el latido tiene un techo del
5 % de escala por la disciplina de §3 — *«si conscientemente ves los nodos latir,
está demasiado fuerte»*.

#### Tres defectos, y el primero sólo existía en la máquina de alguien

**1 · El mapa se dibujaba en un tercio de la ventana.** Dos fallos encadenados
en el tamaño del lienzo, y ninguno se veía en la máquina donde se escribió el
código:

- `resizeTo: window` de Pixi **no aplica en el arranque**: el renderizador se
  quedaba en los 800×600 por defecto.
- `medidas()` calculaba `renderer.width / resolution`, pero en Pixi v8 ese ancho
  **ya viene en píxeles lógicos**. Dividirlo otra vez por la densidad de la
  pantalla lo encogía.

Con densidad 1 el segundo error es invisible —dividir por uno no hace nada—. En
una pantalla escalada al 300 % el mapa salía a un tercio del tamaño, apretado en
la esquina superior izquierda, con el borde del lienzo a la vista y las
etiquetas amontonadas unas sobre otras. Es el defecto que **sólo aparece en la
máquina de otra persona**, y lo destapó una captura de pantalla, no un test.

**2 · Las etiquetas repetían el nombre del repositorio doce veces.** El núcleo
etiqueta las carpetas con dos segmentos porque cinco carpetas llamadas `src` son
cinco nodos indistinguibles — pero cuando el padre es común a casi todas, esos
dos segmentos no desambiguan, sólo tapan. La regla correcta no es fija: se usa
la hoja, y sólo se añade el padre a las que chocan entre sí. `ui`, `docs`,
`obrain-extract` en vez de `Orchestrator's-Brain/ui`. Y cuando dos etiquetas se
solapan, la de la región menor cede: dos nombres ilegibles informan menos que un
nombre y un punto sin él.

**3 · Si la ventana se abría sin el foco del sistema**, el asentamiento nunca
arrancaba y el mapa se quedaba al 25 % de opacidad para siempre — porque el
bucle que lo subía era el mismo que el reposo impedía arrancar. La disciplina de
§3 dice que sin animación la aplicación sigue siendo completamente utilizable,
no que se quede traslúcida.

Y una decisión de encuadre: la simulación reparte sobre un disco de radio
`min(ancho, alto)`, así que en una pantalla apaisada el mapa quedaba como una
bola en el centro. Ahora se escala y se centra lo colocado —sin recolocar nada,
la topología ya está decidida— hasta llenar el 80 % del ancho o el 72 % del alto,
con tope de 2,6×: los radios de las estrellas **no** se escalan, porque
codifican cuántos archivos hay dentro y eso no depende del tamaño de la ventana.

#### Lo que la corrección NO cubre

- **Las regiones son carpetas, y una carpeta no es un núcleo de una vida.** Es
  una aproximación honesta y provisional: los núcleos de verdad son entidades
  `Process` y eso es M6. Si M6 no llega, el cielo se queda siendo un explorador
  de archivos bonito.
- **El corpus con el que se ha medido es un repositorio de código**, así que las
  «estrellas» son crates y carpetas de documentación. La dinámica de tamaños
  real —cuatro mil fotos de un viaje frente a doce recibos— no se ha visto
  todavía, y es justo donde el canal de tamaño se gana su sueldo.
- **La raíz cuadrada está elegida por su curva, no medida.**
- **El cielo depende de cómo estén organizadas las carpetas del usuario.** Para
  quien lo tenga todo en `Descargas`, el cielo será una estrella. Es una
  limitación real de usar contención como eje, y otra razón por la que M6
  importa.
- **De las seis animaciones de §3 hay dos.** Asentamiento (#1) y latido (#4).
  Faltan foco, expansión, **trazo** —la que vende el producto, porque *es* la
  explicación— y sedimentación.
- **El layout sigue siendo O(n²) en el hilo principal**, y ahora es el único
  coste que importa: 197 ms a 300 nodos frente a 0,4 ms de fotograma. Sacarlo a
  un worker (§7.4) es lo que permitiría subir el presupuesto sin bloquear nada,
  y es la recomendación concreta que este hito deja escrita.
- **La temperatura se ha visto sobre un rango de dos días.** Sobre años, que es
  para lo que está pensada la curva, no se ha comprobado.

#### Lo que este informe NO cubre

- **La huella de memoria no se ha medido en release.** El proceso de la
  aplicación en build de depuración ocupa **728 MB de memoria privada**, y §7.2
  dice sin rodeos que *«un proceso en reposo a 400 MB es un motivo de
  desinstalación»*. Comparar una build de depuración contra una afirmación de
  producto no es una medición, y por eso el número se da sin conclusión — pero
  se da. Medirlo en release es lo primero de M5b.
- **La fecha de las sesiones se calcula en UTC.** Con el reloj local a las 18:38
  del día 8, UTC ya era día 9, y una sesión de esta tarde sale etiquetada
  *mañana*. Arreglarlo exige una dependencia de zona horaria o FFI, y esa
  decisión no se toma al final de un hito.
- **La rama semántica no se ha ejercitado en la aplicación.** El modelo no está
  descargado en esta máquina, así que lo verificado es el camino léxico y la
  degradación honesta (`Answer::semantic = false`). Que el hilo de carga instale
  el modelo y la barra pase a híbrida está probado en el CLI con `--modelo`, no
  contra la ventana.
- **La estabilidad del layout está probada del lado del núcleo, no del ojo.**
  Que la misma consulta devuelve el mismo subgrafo con las mismas posiciones de
  partida tiene test; que la simulación converja al mismo sitio se apoya en que
  no hay ni una llamada a `Math.random`, y eso es un argumento, no una medición.
- **La lente puede salirse del viewport.** Con 300 nodos la caja medida iba de
  −138 a 971 px en horizontal sobre un lienzo de 800. Como el mapa es sustrato
  atenuado no molesta; en el modo Foco de M5b habrá que encuadrar.
- **Sin tecla global, sin panel de entidad, sin las seis animaciones.** Es M5b, y
  está dicho en el roadmap y no escondido aquí.
- **`correct` no tiene interfaz.** El comando existe y tiene test —la corrección
  gana y no borra lo que corrige— pero nada en pantalla la dispara todavía.
  §4.5 dice que corregir cuesta un clic; hoy cuesta una llamada.

### Informe del primer corpus real (2026-08-10)

Hasta aquí, todo lo medido se había medido contra **este repositorio**: 101
archivos de Rust y Markdown. El informe de M5a ya lo llamó por su nombre —una
muestra de humo— y el prompt de M5b lo puso como el primer problema a resolver.
Esto es lo que salió al ejecutar el pipeline entero contra el disco del usuario.

#### El 86 % de lo indexable era estado de herramientas

Un escaneo en seco del perfil devolvió **25.665 archivos**. Al desglosarlo,
~22.000 estaban en `.codex`, `.vscode`, `.gemini`, `.antigravity` y `.m2`.
`TOOL_DIRS` ya tenía `.cargo` y `.rustup`; sencillamente se escribió antes de que
existieran las herramientas de IA.

**La corrección no fue alargar la lista, y el motivo importa.** Alargarla arregla
hoy y vuelve a romperse con la herramienta del mes que viene, que es exactamente
el techo que M1 dejó anotado. Se añadió una **señal**: *un directorio con punto
que cuelga directamente de la raíz del perfil es estado de un programa, no
contenido del usuario*. Nadie guarda sus fotos en `C:\Users\x\.vacaciones`. Está
acotada a la raíz a propósito — más adentro, `.borradores` sí puede ser del
usuario, y el criterio del módulo es que ante la duda se indexa.

Aparte, los directorios de credenciales (`.ssh`, `.gnupg`, `.aws`, `.azure`,
`.kube`, `.docker`) pasaron a la lista fija y no a la señal: no se indexan estén
donde estén. La puerta de sensibilidad es la última línea, no la primera.

| | Antes | Después |
|---|---:|---:|
| Archivos indexables en el perfil | 25.665 | **3.655** |
| Directorios recorridos | 6.311 | 582 |
| Duración del recorrido | 3,95 s | 0,60 s |

#### El corpus, y lo que el cielo enseñó

Descartados un checkout de `whisper.cpp` (1.885 archivos de terceros: es
`node_modules` a otra escala) y las partidas guardadas, quedaron **1.042 archivos
enteramente del usuario**: 4.026 vectores, 2.394 aristas, 276 concentradores, 44
duplicados, 28 entidades `sensitive` y 14 `vault`. Densidad 2,30 aristas por
archivo, frente a 1,65 con el ruido dentro.

**La pregunta de la visa, contestada sobre papeles reales.** La consulta
«comprobante de ingresos» —cuyas palabras no aparecen en ningún punto del
corpus, BM25 devolvió cero— recuperó por pura semántica, en 21,8 ms y sin red:
estados de cuenta bancarios, `Estados_de_Cuenta_Abuelo_Visa.pdf`,
`Documentos_Identidad_Respaldo_Visa.pdf` y la línea de captura de un apostillado.
Es el §0 funcionando fuera del laboratorio.

**Los puentes aparecieron solos.** 45 de 48 sesiones cruzaron dos regiones:
`Downloads ↔ Capturas de pantalla`, `Orchestrator's-Brain/core ↔ GitHub/…`. Es la
era «Puentes» de [`EXPERIENCE.md`](EXPERIENCE.md) §6 emergiendo con mil archivos,
sin inventar una sola arista.

#### Tres hallazgos que no son visuales, y por eso importan

Estaba asumido, tras M5a, que **el problema estaba en la capa visual y la
arquitectura de debajo se daba por buena**. El corpus real matiza esa frase: dos
de estos tres son del núcleo.

**1 · Las regiones se dimensionan por número de archivos, y eso no es la
importancia.** La región mayor del cielo son **229 capturas de pantalla**. Las
resoluciones del IMSS y los estados de BBVA —o sea, la pregunta de la visa— viven
en regiones de 12 y de 2. Antes de descartar `whisper.cpp` era aún más claro: **21
de las 30 regiones eran subcarpetas de un único repositorio de terceros**, porque
[`sky::regions`](../core/obrain-api/src/sky.rs) parte repetidamente la región
mayor por peso de subárbol, y un árbol de código es profundo, ancho y uniforme.

Lo llamativo es que [`EXPERIENCE.md`](EXPERIENCE.md) §1 ya legisló este canal para
los nodos: *«Tamaño de nodo = densidad de conexiones. Nunca "importancia"
arbitraria: eso sería una opinión disfrazada de dato»*. Las regiones usan otra
regla para el mismo canal visual. No es un umbral que retocar: es una
incoherencia entre dos partes del lenguaje visual, y hay que resolverla en M5b.

Es también la consecuencia que [ADR-0013](adr/0013-el-mapa-es-la-pantalla.md)
dejó escrita sin poder medirla — *«la calidad del cielo depende por completo de
cómo tenga el usuario organizadas sus carpetas»*—. Medida, el fallo es más agudo
de lo que sonaba: no es que un usuario desordenado vea una estrella, es que **un
solo repositorio bien ordenado le roba el cielo a la vida entera**.

**2 · El boilerplate de los PDF contamina los vectores y la procedencia.** Los
cinco primeros resultados de «comprobante de ingresos» puntuaron 0.870, 0.870,
0.870, 0.870 y 0.869: empatados a la milésima. El sistema acierta la *clase* de
documento y dentro de la clase no discrimina. El motivo se ve en el fragmento que
enseña como prueba — un **sello digital del SAT en base64**, y para dos resultados
distintos el mismo texto legal de relleno sobre la inflación, palabra por palabra.

El relleno común a todos los estados de cuenta se trocea y se vectoriza como si
fuera contenido, y al ser el texto más «documental» del archivo, gana la
similitud. Esto choca de frente con **I9** (ninguna respuesta sin procedencia) y
con §4.4, que exige que el «¿por qué?» resalte el fragmento exacto del origen: si
ese fragmento es un sello en base64, la procedencia no explica nada y en pantalla
se leerá como que el programa está roto — justo en el gesto que más confianza
tiene que generar.

Es el mismo patrón que M2 y M3 ya vivieron con las cachés de Chromium: los
contadores decían éxito y auditar el contenido decía otra cosa.

**3 · La vigilancia continua no tenía hito.** Está diseñada en §4.1 con la
tecnología elegida (USN Change Journal + `ReadDirectoryChangesW`), es la razón por
la que [ADR-0003](adr/0003-lenguaje-del-nucleo.md) eligió Rust para *«un demonio
que corre todo el día»*, y [ADR-0007](adr/0007-identidad-tras-un-guardado-atomico.md)
la cita como la solución correcta a largo plazo. No estaba en el roadmap. Se
añade ahora como **M5c**.

Lo que sí funciona hoy, comprobado y no supuesto: reescanear la misma carpeta dio
*0 entidades nuevas, 851 reconocidas, 0 aserciones escritas, 4.697 sin cambio, en
0,04 s*. La parte difícil —que la identidad aguante y que el log no crezca— está
hecha y con tests. Lo que falta es sólo **cómo se entera**: hoy vuelve a recorrer
en vez de que el sistema operativo le avise. Y recorrer el perfil entero cuesta
0,60 s, así que la vigilancia ya es viable con un temporizador; el journal la pasa
de barata a instantánea.

---

## 10. Decisiones pendientes

- ¿El conocimiento sobrevive al archivo que lo originó cuando ese archivo desaparece? (Postura provisional: sí — el nodo es *conocimiento sobre* el archivo, no el archivo.)
- ¿Habrá alguna vez conocimiento compartido entre personas? No implementarlo ahora es correcto; ignorarlo en el modelo de datos, no.
- ¿El sistema escribe alguna vez en los archivos del usuario? (Postura provisional: v1 estrictamente solo-lectura; el derecho a escribir se gana.)
- **¿Se indexa la papelera de reciclaje?** Hoy no: `$Recycle.Bin` está en `SYSTEM_DIRS` de [`obrain-scan/src/exclude.rs`](../core/obrain-scan/src/exclude.rs) y además cae por el atributo oculto/sistema. El usuario ha pedido amplitud de escaneo *«desde mis descargas hasta mi papelera de reciclaje»* (2026-08-10), así que deja de ser una exclusión obvia y pasa a ser una decisión. A favor: es exactamente donde vive lo que no recuerdas que tienes, que es la cuña F3. En contra: contenido que el usuario decidió borrar reapareciendo en las respuestas, con una tensión incómoda con I4 (nada se borra, se retracta) — I4 habla del grafo, y la papelera es el usuario ejerciendo lo contrario sobre sus archivos. Si se decide que sí, necesita ser opt-in explícito y visiblemente marcado en los resultados, no una exclusión menos en una lista.

- ¿El escaneo debería distinguir *«no lo indexé»* de *«no existe»* de cara al usuario? Hoy un archivo podado por `exclude.rs` es indistinguible de uno que nunca estuvo, y con el explorador de [ADR-0014](adr/0014-dos-vistas-un-solo-estado.md) esa diferencia pasa a ser visible: el usuario podrá bajar hasta una carpeta y ver que falta algo.
- Slug técnico: `orchestrator-brain` u `obrain`. El apóstrofo de `Orchestrator's-Brain` dará guerra en scripts, CI, rutas y URLs. **Ya no es una previsión:** M5a fijó `obrain` para paquetes e identificadores —`obrain-ui`, `obrain-app`, `dev.obrain.app`— pero la **carpeta** sigue llevando apóstrofo y ya ha costado un rodeo en `ui/src-tauri/build.rs`. Lo que queda por decidir es sólo eso: si se renombra el directorio. Ver el informe de M5a.

---

## Registro de decisiones

| ADR | Título | Estado |
|-----|--------|--------|
| [0001](adr/0001-log-de-aserciones-como-fuente-de-verdad.md) | Log de aserciones como fuente de verdad | Aceptada |
| [0002](adr/0002-sustrato-de-persistencia.md) | SQLite como sustrato único de persistencia | Aceptada |
| [0003](adr/0003-lenguaje-del-nucleo.md) | Rust para el núcleo de indexación | Aceptada |
| [0004](adr/0004-punto-de-entrada-barra-no-galaxia.md) | La barra de comandos es el punto de entrada; el mapa es el sustrato | **Sustituida** por 0013 |
| [0005](adr/0005-donde-vive-el-texto-extraido.md) | El texto extraído es una proyección, no una aserción | Aceptada |
| [0006](adr/0006-donde-vive-la-sensibilidad.md) | La sensibilidad es una columna impuesta con procedencia en el log | Aceptada |
| [0007](adr/0007-identidad-tras-un-guardado-atomico.md) | La identidad se reconcilia por ruta cuando el `FileID` se rompe | Aceptada |
| [0008](adr/0008-modelo-de-embeddings-y-runtime.md) | `multilingual-e5-small` sobre ONNX Runtime | Aceptada |
| [0009](adr/0009-donde-viven-los-vectores.md) | Los vectores son una proyección y su identidad I7 está en la clave primaria | Aceptada |
| [0010](adr/0010-recorrido-lineal-frente-a-indice-ann.md) | Recorrido lineal en vez de `sqlite-vec`, y el umbral donde deja de valer | Aceptada |
| [0011](adr/0011-rerank-diferido.md) | El rerank se difiere, con la medición que lo justifica | Aceptada |
| [0012](adr/0012-el-nucleo-expone-una-superficie.md) | El núcleo expone una superficie; la aplicación de escritorio es un cliente | Aceptada |
| [0013](adr/0013-el-mapa-es-la-pantalla.md) | El mapa es la pantalla; la barra busca dentro de él | Aceptada — sustituye a 0004 |
| [0014](adr/0014-dos-vistas-un-solo-estado.md) | Dos vistas, un solo estado: la galaxia y el explorador | Aceptada — acota `EXPERIENCE.md` §4.4 |

## Documentos

| Documento | Contenido |
|-----------|-----------|
| [EXPERIENCE.md](EXPERIENCE.md) | Fase 3 — lenguaje visual, animaciones, navegación, arranque en frío, evolución del mapa |
