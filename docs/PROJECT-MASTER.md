# Orchestrator's Brain — Documento Maestro

> Las aplicaciones actuales almacenan información. Orchestrator's Brain construye conocimiento.

**Estado:** Replanteamiento — la bóveda · **Última revisión:** 2026-09-22 · **Documento vivo**

Este documento refleja siempre el estado *actual* del diseño. Se edita libremente.
El *porqué* de las decisiones contestadas vive en [`adr/`](adr/) y no se edita nunca.
**Dónde estamos y qué sigue, en una página:** [`ESTADO.md`](ESTADO.md). Los informes de
cada hito de agosto están en la [bitácora](archivo/bitacora-agosto-2026.md).

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

**Implementada (2026-08-07)**, antes de que exista un solo vector. `gate::decide` es esta tabla como función pura; sus tests son la tabla. Falla cerrado: una entidad desconocida se trata como `vault`. La búsqueda léxica queda deliberadamente fuera de la puerta —la tabla ya se la concede a `vault`— porque BM25 es determinista, local y no deja un derivado del que se pueda reconstruir el original. Un vector sí, y por eso sí pasa. El nivel se clasifica con señales de nombre y de contenido, y vive según [ADR-0006](adr/0006-donde-vive-la-sensibilidad.md). Ver la [bitácora](archivo/bitacora-agosto-2026.md).

### 5.3 Migración de embeddings (I7)

Todo vector guarda `(model_id, model_version, dim, chunker_version)`. Una migración es: crear el índice nuevo en paralelo, re-embeber en segundo plano con presupuesto, conmutar cuando la cobertura sea suficiente, retirar el viejo. Sin esto, cambiar de modelo de embeddings a los dos años es un evento de extinción.

**Implementado (2026-08-07)** por [`core/obrain-embed`](../core/obrain-embed). Los cinco datos están **en la clave primaria** de `vectors`, que es lo que hace que «en paralelo» sea posible y no una intención: con la clave puesta en el fragmento, el segundo modelo pisaría al primero y la migración dejaría de ser gradual. Ver [ADR-0009](adr/0009-donde-viven-los-vectores.md) y la [bitácora](archivo/bitacora-agosto-2026.md).

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

### 7.3 UI: Tauri v2 + Svelte 5

- Frente a Electron: ~10 MB contra ~150 MB, y usa el webview del sistema. Para una app permanentemente abierta, la diferencia de memoria decide.
- **Coste honesto:** Tauri usa WebView2 (Chromium) en Windows pero WebKit en Linux/macOS, con lo que el renderizado difiere entre plataformas. Como la v1 es sólo Windows, el coste está diferido — pero está, y hay que decirlo ahora y no descubrirlo el día del port.
- Frente a nativo (WinUI 3): mejor techo de rendimiento, pero renuncia a todo el ecosistema web de visualización, que es justo lo que necesita la galaxia. No compensa.
- **Dentro del webview, Svelte 5** (decidido por el usuario el 2026-09-22, aplicado en la Fase 2 ese mismo día). La primera interfaz se escribió en TypeScript sin marco, construyendo cada elemento a mano, y eso era buena parte de por qué se sentía rígida: sin componentes no hay transiciones ni piezas reutilizables. El mapa de PixiJS pasó casi tal cual como un componente (ahora se mide por su contenedor y se destruye al salir de la vista). El estado vive en una sola clase con runas (`ui/src/lib/app.svelte.ts`); los iconos son `@lucide/svelte` importados de uno en uno, y las tipografías (Geist, Geist Mono, Instrument Serif) van empaquetadas con `@fontsource`: **nada se pide a internet al abrir la app**. El tema —noche y día— son variables CSS en `ui/src/estilos/tema.css`.
- **El webview no recibe permisos nuevos.** «Añadir carpeta» abre el selector de carpetas desde Rust (`tauri-plugin-dialog`) y el webview sólo pide *«quiero añadir una»*: nunca manda una ruta, igual que abrir y revelar reciben un identificador de entidad y no una ruta (§10, M5b).

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

> **Estado real (2026-09-22): el cifrado en reposo y la clave de bóveda no están
> implementados.** La base es un SQLite **sin cifrar** en `%LOCALAPPDATA%\obrain`.
> Lo que sí existe son los tres niveles de sensibilidad y la puerta de egreso
> (§5.2), que impiden que lo sensible salga de la máquina, y que el contenido
> sensible no se enseñe hasta pedirlo. Cifrar es la Fase 3, y tiene que llegar
> antes de guardar un pasaporte, no después.

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
| ✅ **M1 — Ver el disco** | Descubrimiento, identidad, triaje de placeholders, exclusiones. → [`core/obrain-scan`](../core/obrain-scan) | **Hecho (2026-08-07).** Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ✅ **M2 — Leer el disco** | Extracción por formato, chunking versionado, FTS5. → [`core/obrain-extract`](../core/obrain-extract) | **Hecho (2026-08-07).** 5.015 fragmentos, búsqueda BM25 en 0,5–1,9 ms. Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ✅ **M3 — Relacionar sin IA** | Aristas deterministas de §4.4. → [`core/obrain-link`](../core/obrain-link) | **Hecho (2026-08-07).** 2.480 aristas, 0 llamadas a un modelo. Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| 🔒 **Puerta de sensibilidad** | Clasificación `normal`/`sensitive`/`vault` y aplicación de §5.2. → [`core/obrain-guard`](../core/obrain-guard) | **Hecha (2026-08-07).** No es un hito: es el requisito previo de M4. Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ✅ **M4 — Entender** | Embeddings locales, búsqueda semántica, fusión RRF. → [`core/obrain-embed`](../core/obrain-embed) | **Hecho (2026-08-07).** 2.479 vectores, `universidad` encuentra un formulario de matrícula que no contiene la palabra. Rerank **diferido con la medición** ([ADR-0011](adr/0011-rerank-diferido.md)). Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ✅ **M5a — La barra sobre el sustrato** | Superficie del núcleo, lente de I2, barra de ADR-0004. → [`core/obrain-api`](../core/obrain-api) + [`ui`](../ui) | **Hecho (2026-08-08).** Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ✅ **M5b — Llegar y ver** | Dos cosas: el conmutador galaxia/explorador de [ADR-0014](adr/0014-dos-vistas-un-solo-estado.md), y el panel de entidad de [`EXPERIENCE.md`](EXPERIENCE.md) §4.4 **con previsualización**. Nada más. | **Hecho (2026-08-13).** Se abre la app, se baja hasta el PDF de la visa sin escribir su nombre, y se lee — con el párrafo que lo explica marcado. Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ✅ **M5d — Manejarlo** | La cámara del mapa —zoom y arrastre— y saber qué nodo vas a abrir antes de abrirlo. Sale de **usar M5b**, no de una lista. → [`m5d-manejarlo.md`](archivo/prompts/m5d-manejarlo.md) | **Hecho (2026-08-14).** Te acercas a donde quieras del mapa, y sabes qué vas a abrir antes de abrirlo. Ver la [bitácora](archivo/bitacora-agosto-2026.md). |
| ◐ **M5c — Enterarse** | ~~USN Change Journal para el volumen~~ — con una bóveda elegida no hace falta vigilar el disco entero ([ADR-0018](adr/0018-bibliotecas.md)). | **Parcial (2026-09-22):** la aplicación reindexa tus bibliotecas al arrancar y con el botón «actualizar», en segundo plano. Falta la vigilancia en vivo de la carpeta (`ReadDirectoryChangesW` vía `notify`). |
| **M6 — Construir conocimiento** | Extracción de entidades, detección de `Process`, enriquecimiento por atención. | La métrica de §0 es medible y positiva. |
| **M7 — La pregunta** | Comprensión de consulta, resolución temporal, respuesta con citas. | **Responde la pregunta de la visa.** |

M3 es el hito más subestimado y probablemente el que más valor por hora de trabajo entrega. M7 es el que valida la tesis.

### El replanteamiento (2026-09-22): de «todo mi disco» a «mi bóveda»

Tras cinco semanas sin tocar el proyecto se revisó entero, con la aplicación
funcionando contra un corpus de documentos personales. El motor está sano —266
tests, un pipeline que funciona de punta a punta—; lo que no encajaba era el
**objetivo**. El diseño de M1–M5 era *entender todo el disco del usuario*
(OneDrive, Descargas, sesiones de trabajo). Lo que el usuario quiere ahora es
**una bóveda de documentos personales que él elige uno a uno**: pasaporte,
seguros médicos, matrícula, contratos. Es una simplificación, no una renuncia:
buena parte de lo difícil (vigilar el volumen, filtrar la basura de las
herramientas, el triaje de OneDrive a escala) pasa a importar menos, y otras
cosas pasan a ser el centro (OCR, nombres limpios, vencimientos, cifrado).

| Fase | Contenido | Estado |
|---|---|---|
| **0 — Limpieza** | Este documento de 2.008 a ~450 líneas (los informes, a la [bitácora](archivo/bitacora-agosto-2026.md)); [`ESTADO.md`](ESTADO.md); README honesto sobre el cifrado. | **Hecha (2026-09-22)** |
| **1 — El núcleo para tu carpeta** | Bibliotecas ([ADR-0018](adr/0018-bibliotecas.md)): el árbol empieza en la carpeta elegida; las carpetas se llaman por su nombre; `obrain indexar <carpeta>` hace el pipeline entero; la aplicación reindexa sola y tiene «actualizar»; fechas y tamaños legibles; el explorador ya no aplasta sus columnas. | **Hecha (2026-09-22)** |
| **2 — La interfaz nueva** | Svelte 5. El explorador como escritorio de trabajo, estilo sistema operativo: barra lateral con tus carpetas, Inicio (recientes, tu mapa en pequeño), carpetas en cuadrícula o lista con miniaturas, vista previa a la derecha con lo técnico plegado, una vista de Sensibles; búsqueda con `Ctrl+K` o escribiendo; la galaxia como vista «Mapa» enlazada en los dos sentidos; aspecto de noche y de día; «Añadir carpeta» desde la app. En el núcleo, `Brain::recent` y `Brain::sensitive`. | **Hecha (2026-09-22).** La maqueta para elegir el aspecto está hecha pero el usuario aún no la ha revisado: se construyó con la propuesta (explorador de inicio, noche por defecto) y queda confirmarla (§10). |
| **3 — Documentos personales** | OCR (el de Windows, local); vencimientos (*¿qué me caduca pronto?*); sensibilidad por carpeta —en el banco de pruebas, las fotos de la TIE no salieron sensibles porque la puerta sólo mira nombres y texto, y una foto no tiene texto—; cifrado en reposo y PIN. | Siguiente |
| **4 — La visión** | M6 y M7: trámites como `Process`, la pregunta de la visa. | Congelada hasta la 3 |

**Lo que se congela** —no se borra, no se sigue—: sesiones y puentes, versiones,
el USN Change Journal, sincronización, multi-tenant.

---

## 10. Decisiones pendientes

- ¿El conocimiento sobrevive al archivo que lo originó cuando ese archivo desaparece? (Postura provisional: sí — el nodo es *conocimiento sobre* el archivo, no el archivo.)
- ¿Habrá alguna vez conocimiento compartido entre personas? No implementarlo ahora es correcto; ignorarlo en el modelo de datos, no.
- ¿El sistema escribe alguna vez en los archivos del usuario? (Postura provisional: v1 estrictamente solo-lectura; el derecho a escribir se gana.)
- ~~**¿Qué pasa exactamente al tocar un nodo: previsualizar, abrir, o las dos?**~~ **Decidido por el usuario el 2026-08-13: las tres cosas.** Previsualizar dentro (texto ya extraído e imágenes locales), abrir con el programa del sistema, y revelar en la carpeta. Las dos últimas son la única ampliación de capacidades de la aplicación y van por comandos de Rust que reciben **un identificador de entidad, nunca una ruta**: un comando que aceptara rutas sería un «abre esto» genérico expuesto al webview. La página de PDF renderizada queda fuera —necesita un motor nativo y el texto ya está extraído desde M2—. Ver el informe de M5b.
- ~~**La previsualización choca con el criterio de M1.**~~ **Resuelto en M5b (2026-08-13).** El texto sale de `chunks` y no abre ningún archivo, así que no necesita guarda; la miniatura sí abre, y por eso `Availability::may_read_content` va delante de `File::open` y se importa de `obrain-scan` en vez de reescribirse. Probado contra un `.docx` real de OneDrive: `NO SE ABRE`. El diagnóstico se conserva porque sigue siendo la razón de que el orden de esos dos caminos importe.
- **¿Un PIN para ver lo sensible?** Planteado por el usuario el 2026-08-13: *«estaría bien ya más al final definir todas aquellas cosas que sean sensibles y si las quieres ver que te pida algún pin de 4 dígitos o algo así, por más que sea una app solo para mí»*. **No es una pieza nueva:** §8.1 ya dice que `vault` lleva clave separada derivada de una frase de paso que no se almacena, y §5.2 ya clasifica en tres niveles. Un PIN es la **interfaz** de ese mecanismo, y hoy sólo existe la mitad barata —M5b oculta el contenido hasta un clic, que es un gesto, no una llave—. Lo que hay que decidir: si el PIN protege *ver* o protege *descifrar* (son cosas muy distintas: lo segundo obliga a que la clave de bóveda no esté en DPAPI, que es lo que §8.1 ya prevé); si el usuario puede marcar a mano qué es sensible, lo que lo emparenta con las regiones de usuario y con I5; y qué pasa cuando se olvida el PIN, que en un diseño donde la frase de paso no se almacena tiene una única respuesta honesta y hay que decirla antes, no después.
- **¿Puede el usuario crear sus propias regiones y nombrarlas?** Pedido el 2026-08-10: *«quiero poder ordenar mis cosas a mi gusto y que el programa no pierda de vista qué es qué y con qué se relaciona»*. La mitad difícil ya está garantizada por **I5** —una aserción del usuario gana para siempre y ningún reescaneo la pisa— y `Brain::correct` ya la escribe con test. Lo que no está decidido es lo estructural: hoy las regiones salen del árbol de contención del disco, y esto introduce **una segunda fuente de regiones que no es el sistema de archivos**, conviviendo con la primera. Hay que decidir si una región de usuario es una entidad más con una faceta, cómo se resuelve que un archivo caiga en dos, y qué manda al dibujar el cielo. Está emparentado con el hallazgo del dimensionado de regiones del informe del corpus real: si el usuario puede agrupar a mano, el peso por número de archivos deja de ser la única respuesta posible.
- **¿Se indexa la papelera de reciclaje?** Hoy no: `$Recycle.Bin` está en `SYSTEM_DIRS` de [`obrain-scan/src/exclude.rs`](../core/obrain-scan/src/exclude.rs) y además cae por el atributo oculto/sistema. El usuario ha pedido amplitud de escaneo *«desde mis descargas hasta mi papelera de reciclaje»* (2026-08-10), así que deja de ser una exclusión obvia y pasa a ser una decisión. A favor: es exactamente donde vive lo que no recuerdas que tienes, que es la cuña F3. En contra: contenido que el usuario decidió borrar reapareciendo en las respuestas, con una tensión incómoda con I4 (nada se borra, se retracta) — I4 habla del grafo, y la papelera es el usuario ejerciendo lo contrario sobre sus archivos. Si se decide que sí, necesita ser opt-in explícito y visiblemente marcado en los resultados, no una exclusión menos en una lista.

- **¿Observatorio o Instrumento?** **Se le enseñaron las dos el 2026-08-14 y pidió verlas moviéndose antes de decidir** — que es la respuesta correcta y no un aplazamiento: la diferencia entre las dos es de tratamiento de la luz, y la luz de este mapa cambia al acercarse. Ahora que la cámara existe, la comparación se puede hacer con el mapa en la mano en vez de con una captura. La dirección visual sigue sin elegir, y es del usuario, no del sistema. Los tres prototipos están en [`prototypes/02-direcciones`](../prototypes/02-direcciones/index.html); Cartografía quedó descartada de hecho al elegir entre las otras dos. Aplazado a propósito hasta después de M5b: las dos finalistas comparten geometría y navegación, así que la diferencia es de tratamiento —luz, contención, adorno— y elegir paleta antes de que el explorador y el panel cambien las proporciones de la pantalla es maquillar una composición que todavía se mueve. La referencia de la que salió la intención visual está guardada en [`docs/referencias/`](referencias/), con la tensión anotada: la imagen sube la saturación, y §1 le tiene asignado un significado —codifica **confianza**—, así que subir el suelo estrecha el rango disponible para decir «esto es una inferencia dudosa». Si se sube, el margen se reserva antes, no después.
- **¿Cómo se representa que un documento está *dentro* de otro?** Aparecido al auditar la prueba el 2026-08-12 ([ADR-0016](adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md)). En el corpus real, `Estados_de_Cuenta_Abuelo_Visa.pdf` contiene los tres estados de cuenta mensuales del banco, y `Documentos_Identidad_Respaldo_Visa.pdf` contiene otro — comprobado por hash, fragmento a fragmento, 6 de 6 idénticos al byte. A escala del corpus son **467 fragmentos, el 11,5 % del índice y de los vectores, en 44 documentos**. Hoy el sistema no tiene forma de decirlo: M3 modela `duplicate_of` para archivos enteros (44 en este corpus) pero no *«este documento es parte de aquel»*, y sin esa arista una búsqueda devuelve cinco filas donde hay tres documentos, sin poder explicar por qué se parecen tanto. Lo que hay que decidir: si es un predicado nuevo del vocabulario de §3.4 o un caso de los que ya hay; si lo detecta M3 al enlazar (comparando hashes de fragmento, que es determinista y barato) o M6 al inferir; y qué hace la interfaz con ello — colapsar las filas es lo obvio, pero **la razón por la que se colapsan es justo el tipo de cosa que un gestor de archivos no puede enseñar y este proyecto sí**, así que esconderlo sin más sería desaprovecharlo. Es contenido legítimo duplicado, no ruido: no se filtra.
- ¿El escaneo debería distinguir *«no lo indexé»* de *«no existe»* de cara al usuario? Hoy un archivo podado por `exclude.rs` es indistinguible de uno que nunca estuvo, y con el explorador de [ADR-0014](adr/0014-dos-vistas-un-solo-estado.md) esa diferencia pasa a ser visible: el usuario podrá bajar hasta una carpeta y ver que falta algo.
- **¿Qué es la pantalla principal: el explorador o la galaxia?** Propuesta del 2026-09-22. El usuario aclaró que la galaxia sigue siendo interactiva, y la Fase 2 se construyó así **a falta de su confirmación final**, que dará al revisar la maqueta y la app (también si el aspecto por defecto es el de noche o el de día): **el explorador es el escritorio de trabajo** —donde se abre la aplicación y donde se busca, se abre y se previsualiza a diario— y **la galaxia es una vista propia, interactiva**, a la que se va para explorar y descubrir relaciones, enlazada en los dos sentidos (pulsar una estrella lleva a esa carpeta en el explorador; «ver en el mapa» lleva de un documento a la galaxia). En el inicio aparece en pequeño, como una tarjeta que lleva al mapa. Contradice a [ADR-0013](adr/0013-el-mapa-es-la-pantalla.md) y, si se confirma, necesita su propio ADR que lo sustituya.
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
| [0015](adr/0015-el-cielo-reparte-por-anchura-no-por-peso.md) | El cielo reparte por anchura; el tamaño sigue siendo cuántos archivos hay | Aceptada |
| [0016](adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md) | La prueba se elige; el índice no se filtra | Aceptada — corrige el hallazgo 2 de §9 |
| [0017](adr/0017-la-barra-se-pliega.md) | La barra se pliega a un icono, y escribir la despliega | Aceptada — conserva el argumento de 0004 |
| [0018](adr/0018-bibliotecas.md) | Bibliotecas: el árbol empieza en la carpeta que eliges, y cada carpeta se llama por su nombre | Aceptada |

## Documentos

| Documento | Contenido |
|-----------|-----------|
| [ESTADO.md](ESTADO.md) | **Empieza aquí.** Dónde está el proyecto, cómo se usa y qué sigue, en una página |
| [EXPERIENCE.md](EXPERIENCE.md) | Fase 3 — lenguaje visual, animaciones, navegación, arranque en frío, evolución del mapa |
| [archivo/bitacora-agosto-2026.md](archivo/bitacora-agosto-2026.md) | Los informes de M1 a M5d, tal como se escribieron |
| [archivo/prompts/](archivo/prompts/) | Los prompts de diseño usados durante agosto |
