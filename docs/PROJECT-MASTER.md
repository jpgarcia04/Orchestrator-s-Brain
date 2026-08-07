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

### 5.2 Política de egreso (I11)

Cada entidad tiene un nivel de sensibilidad; cada invocación declara si puede salir de la máquina. El router **rechaza** enviar una entidad `vault` a un proveedor remoto. No es una preferencia de configuración: es una comprobación aplicada en la capa de routing y verificable con tests.

| Nivel | Índice vectorial | Egreso a modelo remoto | Sync remoto |
|-------|------------------|------------------------|-------------|
| `normal` | Sí | Sí, con consentimiento | Contenido cifrado |
| `sensitive` | Sí, local | Sólo modelos locales | Contenido cifrado |
| `vault` | Sólo léxico local | Nunca | Sólo metadatos |

### 5.3 Migración de embeddings (I7)

Todo vector guarda `(model_id, model_version, dim, chunker_version)`. Una migración es: crear el índice nuevo en paralelo, re-embeber en segundo plano con presupuesto, conmutar cuando la cobertura sea suficiente, retirar el viejo. Sin esto, cambiar de modelo de embeddings a los dos años es un evento de extinción.

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
| Vectores | `sqlite-vec`, componente reemplazable — **proyección** |
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
| **M1 — Ver el disco** | Descubrimiento, identidad, triaje de placeholders, exclusiones. | Indexa una máquina real sin hidratar OneDrive ni indexar `node_modules`. |
| **M2 — Leer el disco** | Extracción por formato, chunking, FTS5. | Búsqueda léxica instantánea sobre el corpus completo. |
| **M3 — Relacionar sin IA** | Todas las aristas deterministas de §4.4. | El grafo tiene densidad real y **cero** llamadas a un modelo. |
| **M4 — Entender** | Embeddings locales, búsqueda semántica, fusión RRF, rerank. | Encuentra por significado, no por nombre. |
| **M5 — La galaxia** | Lente, layout estable, navegación por foco. | Se puede explorar un `Process` y sus artefactos visualmente. |
| **M6 — Construir conocimiento** | Extracción de entidades, detección de `Process`, enriquecimiento por atención. | La métrica de §0 es medible y positiva. |
| **M7 — La pregunta** | Comprensión de consulta, resolución temporal, respuesta con citas. | **Responde la pregunta de la visa.** |

M3 es el hito más subestimado y probablemente el que más valor por hora de trabajo entrega. M7 es el que valida la tesis.

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

## Documentos

| Documento | Contenido |
|-----------|-----------|
| [EXPERIENCE.md](EXPERIENCE.md) | Fase 3 — lenguaje visual, animaciones, navegación, arranque en frío, evolución del mapa |
