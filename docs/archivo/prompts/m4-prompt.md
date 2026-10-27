# Prompt para M4 — Entender

Copia todo lo de abajo en una conversación nueva de Claude Code, con working directory en este repo.

---

Estoy construyendo **Orchestrator's Brain**, un motor de conocimiento personal local-first. La arquitectura y las decisiones ya están cerradas y documentadas — léelas antes de escribir nada:

1. [`docs/PROJECT-MASTER.md`](../../PROJECT-MASTER.md) — documento vivo. Lee especialmente §5 (capa de IA), §6 (búsqueda y recuperación), §7.1 (persistencia) y **los cuatro informes del final**: M1, M3, M2 y el de la puerta de sensibilidad.
2. `docs/adr/0001` a `0006` — decisiones fundacionales con sus alternativas descartadas. `0005` (dónde vive el texto) y `0006` (dónde vive la sensibilidad) son las dos que M4 toca directamente.
3. [`docs/EXPERIENCE.md`](../../EXPERIENCE.md) — no aplica a M4, pero da contexto de hacia dónde va esto.

**Código ya existente y funcionando** (léelo, no lo reescribas):

- [`core/obrain-core`](../../../core/obrain-core) — M0. Log de aserciones bitemporal. `Store`, `Assertion`, `Method`, `Facet`, `Sensitivity`, `TimePoint`. I1/I4 impuestos por triggers de SQLite.
- [`core/obrain-scan`](../../../core/obrain-scan) — M1. Descubrimiento, identidad `(volumen, FileID)` + BLAKE3, triaje de placeholders de OneDrive, exclusiones.
- [`core/obrain-link`](../../../core/obrain-link) — M3. Enlazadores deterministas. 2.534 aristas sin una sola llamada a IA.
- [`core/obrain-extract`](../../../core/obrain-extract) — M2. Extracción por formato, `paragraph-pack@1`, tablas `extractions`/`chunks`/`chunks_fts`, búsqueda BM25 en `search.rs`.
- [`core/obrain-guard`](../../../core/obrain-guard) — la puerta. Clasifica `normal`/`sensitive`/`vault` y aplica §5.2. **Es la dependencia dura de este hito.**

Todo compila con `cargo build --manifest-path core/Cargo.toml`. Toolchain MSVC ya instalado. 145 tests en verde, clippy limpio con `-D warnings`.

## Qué es M4

Del roadmap en `PROJECT-MASTER.md` §9:

> **M4 — Entender.** Embeddings locales, búsqueda semántica, fusión RRF, rerank. Criterio de "hecho": **encuentra por significado, no por nombre.**

M4 es el primer hito con un modelo dentro. Es también el hito de la decisión más difícil de deshacer del proyecto entero, y el maestro lo dice sin rodeos en el invariante I7: *«los embeddings son el lock-in real, no el LLM»*.

## El hardware, para que no lo descubras midiendo

Esta máquina: **Intel i5-13420H, 16 GB de RAM, NVIDIA RTX 4050 Laptop (6 GB)**.

Es de sobra para M4 y **no hace falta la GPU**. El corpus son 5.229 fragmentos y 9,0 M de caracteres: un modelo de embeddings pequeño en CPU los procesa en minutos. Añadir CUDA a la ecuación en este hito es complejidad de despliegue a cambio de nada. Si al medir resulta que la CPU no llega, dilo con números antes de tocar la GPU.

## Alcance de M4

Crear un nuevo crate `core/obrain-embed`:

### 1. La capacidad `Embedder`

Un contrato como el de §5.1, no un envoltorio de una API concreta: `text[] → vector[]`. Detrás, un modelo local.

Sobre la elección del modelo, mi recomendación —que tienes que verificar, no obedecer—:

- El corpus real es **castellano, inglés y código fuente mezclados**. Un modelo monolingüe inglés queda descartado por eso solo.
- Candidato por defecto: **`multilingual-e5-small`** (384 dimensiones, ~118 M de parámetros, ~120 MB cuantizado a int8). Tiene una trampa que hay que respetar o los resultados salen mediocres sin que se note por qué: exige los prefijos `query: ` y `passage: ` en la entrada, y son distintos para la consulta y para el documento.
- Alternativas a considerar y descartar por escrito: `paraphrase-multilingual-MiniLM-L12-v2` (384 dims, más antiguo), `bge-m3` (1.024 dims, 2,2 GB — mejor calidad y probablemente demasiado peso para este hito).
- Runtime: **`ort`** (ONNX Runtime) + **`tokenizers`** es la vía madura en Rust hoy. `fastembed-rs` envuelve ambas y es más cómodo, pero descarga los modelos de HuggingFace en tiempo de ejecución — ver la restricción de red más abajo. `candle` es la opción Rust puro y suele ser más lento en CPU; si lo eliges, justifícalo con una medición.

### 2. Persistencia de vectores con I7 cableado

Todo vector guarda `(model_id, model_version, dim, chunker, chunker_ver)`. No es opcional y no es para el futuro: es lo que permite que cambiar de modelo dentro de dos años sea un `rebuild` en paralelo y no un evento de extinción. `extractions` y `chunks` ya llevan `(chunker, chunker_ver)` en la clave primaria por este mismo motivo; sigue el patrón.

Los vectores son una **proyección** (I1, ADR-0005): tabla propia, reconstruible, se puede tirar entera.

### 3. El índice vectorial

§7.1 nombra `sqlite-vec` como «componente reemplazable». Verifica que enlaza con el SQLite `bundled` que ya usa `rusqlite`; si pelea, **mídelo antes de rendirte y antes de insistir**: con 5.229 vectores de 384 dimensiones son 8 MB, y un recorrido lineal completo con producto escalar es del orden de milisegundos. A esta escala un índice ANN no acelera nada y sí añade una dependencia nativa.

Si acabas eligiendo el recorrido lineal, **es una decisión de arquitectura y va en un ADR**, con el umbral de corpus a partir del cual deja de valer.

### 4. Recuperación híbrida y fusión RRF

El pipeline de §6, hasta donde llegue este hito:

```
consulta
  ├── BM25            ← ya existe, en obrain-extract::search
  └── kNN vectorial   ← lo que añade M4
        ↓
  fusión Reciprocal Rank Fusion   score = Σ 1/(k + rank_i),  k = 60
        ↓
  (rerank)
        ↓
  resultados con procedencia
```

RRF y no una media ponderada: las puntuaciones de BM25 y las de coseno no son comparables ni normalizándolas, y unos pesos ajustados a mano sobre este corpus estarían sobreajustados a él. El maestro ya lo decidió; lo que te toca es implementarlo y enseñar en el informe qué cambia respecto a BM25 solo.

### 5. Rerank

Un cross-encoder pequeño sobre el top-50. Es la parte que más puede costar: `bge-reranker-v2-m3` son 568 M de parámetros y en CPU puede tardar más de lo que M2 estableció como listón («instantánea»).

**Mídelo y decide con el número delante.** Si no cabe en el presupuesto de latencia, **diferirlo a un hito posterior es una respuesta aceptable siempre que quede escrita en el informe con la medición que la justifica.** Lo que no es aceptable es incluirlo sin medir o quitarlo sin decirlo.

### 6. CLI

`obrain-embed <db> [--sample] [--buscar <texto>] [--limite <n>]`, en el mismo estilo que `obrain-scan`, `obrain-link`, `obrain-extract` y `obrain-guard`: se ejecuta sobre una base ya preparada, informe final legible, aritmética comprobada.

## Restricciones que no son negociables

- **El origen del índice vectorial es `gate::embeddable(conn)`, nunca `SELECT DISTINCT entity FROM chunks`.** La diferencia entre las dos consultas es exactamente la diferencia entre cumplir I11 y decir que se cumple. Toda invocación del modelo pasa además por `gate::check(store, entity, Capability::Embedder, Runs::Local)`.
- **Escribe un test que lo verifique**: una entidad `vault` con fragmentos no puede acabar con un solo vector. `obrain-guard` ya tiene uno equivalente (`embeddable_excluye_la_boveda`); el tuyo debe atacar tu pipeline, no la puerta.
- **Cero acceso al disco de archivos del usuario.** El texto ya está en `chunks`. M4 no abre ni un archivo, y por tanto no puede hidratar OneDrive. Mantén esa propiedad: es gratis ahora y cara de recuperar.
- **La red se toca una vez, para bajar el modelo, y de forma explícita.** F1 dice local-first: después de esa descarga, todo el pipeline tiene que funcionar sin conexión. Si eliges una librería que descarga modelos de forma implícita en la primera ejecución, envuélvelo en un paso deliberado y dilo en el informe. No es un detalle de comodidad: es la promesa central del producto.
- **Nada de retractar ni tocar aserciones ajenas.** Los vectores son proyección; al log va sólo la procedencia (qué modelo, qué versión, qué cobertura), igual que hizo M2 con la extracción.
- **`cargo clippy --all-targets -- -D warnings` limpio** antes de dar nada por terminado.
- **Los 145 tests existentes en verde.** No deben romperse.

## Dos problemas abiertos que te van a morder si no los miras primero

Los dos están documentados en los informes del maestro y los dos empeoran justo en este hito.

### El 55 % del índice léxico no lo escribió una persona

El informe de M2 lo cuantificó: **2.767 de 5.015 fragmentos salían de `Rockstar Games\…\Renderer` (cargas de extensiones de Chromium) y de `WindowsPowerShell\Modules` (módulos de terceros)**. En BM25 eso es ruido molesto. En búsqueda semántica es peor, porque son texto real y bien formado en inglés: van a aparecer como vecinos plausibles de casi cualquier cosa, y además cuestan cómputo de embeddings.

La recomendación concreta de M2 sigue sin aplicarse: añadir `WindowsPowerShell\Modules` y `PowerShell\Modules` a las exclusiones de `obrain-scan`. **Hazlo antes de embeber** y reporta el corpus antes y después. Es media hora y cambia la calidad de todo lo que midas después.

### Entidades fantasma por cambio de `FileID`

El informe de la puerta lo destapó: guardar un archivo con un editor que escribe en temporal y renombra **le cambia el `FileID` de NTFS**, así que `obrain-scan` lo ve como un archivo nuevo. Queda la entidad vieja con la misma ruta, el contenido caducado y sus fragmentos.

Para M4 esto no es cosmético: son **dos juegos de vectores del mismo documento**, y la búsqueda semántica devolverá el archivo dos veces, uno de ellos con texto que ya no existe. Mide cuántas rutas tienen más de una entidad viva antes de embeber:

```sql
SELECT object_text, COUNT(DISTINCT subject) n
  FROM assertions
 WHERE predicate = 'path_observed_at' AND retracted_at IS NULL
 GROUP BY object_text HAVING n > 1;
```

Si son pocas, repórtalo y sigue. Si son muchas, la reconciliación por `(ruta + hash)` en `obrain-scan` pasa a ser prerrequisito. En cualquier caso **el número va en el informe**.

## Metodología que ya demostró su valor cuatro veces — repítela

En M1, M2, M3 y en la puerta, los defectos reales **no aparecieron en los contadores**. Aparecieron mirando resultados de verdad: 369 fotos encadenadas como versiones, 12 archivos ocupando el 62 % del índice, tres `.env.example` en la bóveda, un NIE en la propia documentación del repositorio. Cada uno se volvió un test de regresión con el caso real que lo destapó.

Para M4 eso significa:

1. **Ejecuta contra el corpus real de esta máquina** (`%USERPROFILE%\OneDrive`), no sólo contra datos sintéticos.
2. **`--sample` es parte del hito.** Como mínimo tiene que enseñar: los vecinos más cercanos de un puñado de documentos reales; los fragmentos que aparecen como vecinos de demasiadas cosas (los *hubs*, que son la patología clásica de un espacio vectorial y que ningún contador delata); y la comparación lado a lado de BM25, kNN y RRF para la misma consulta.
3. **Verifica la aritmética** como en los cuatro informes anteriores: si dices «N fragmentos vectorizados», comprueba que N cuadra con los fragmentos elegibles menos los retenidos por la puerta (hoy 5.229 − 28).
4. **Si la calidad es mala, dilo con ejemplos.** Un informe que diga «la búsqueda semántica funciona» sin enseñar una consulta que falla no es un informe, es una nota de prensa.

## El criterio de "hecho", en concreto

No es «hay vectores en la base». Es **encontrar por significado algo que BM25 no encuentra**. Construye la prueba así:

- Una consulta cuyas palabras **no aparecen** en el documento objetivo, y que aun así lo devuelve.
- La comparación honesta contra BM25 para esa misma consulta, enseñando que BM25 no lo encuentra.
- Y un ensayo del test canónico del proyecto, aunque todavía no pueda pasarlo entero: *«el documento que usé para demostrar ingresos»*. M4 no puede resolver «durante mi trámite de visa» —eso es resolución de entidad y rango temporal, y es M7—, pero la mitad semántica de la frase ya debería empezar a funcionar. Enseña hasta dónde llega y dónde se rompe.

Ten en cuenta al medir que los 14 documentos de Office deshidratados en OneDrive —presupuestos, actas, revisiones— **siguen sin texto extraído**, y son el material del corpus que más se parece a la pregunta de la visa. No los cuentes como fallo de M4.

## Al terminar

1. Actualiza `docs/PROJECT-MASTER.md`: marca M4 como hecho en §9 con un informe tan riguroso como los cuatro anteriores — números reales, aritmética verificada, defectos encontrados y corregidos, y una sección explícita de lo que el informe **no** cubre.
2. Documenta como ADR nuevo en `docs/adr/` toda decisión de arquitectura no trivial: qué modelo y por qué, dónde viven los vectores, `sqlite-vec` frente a recorrido lineal, si se difiere el rerank. Sigue el formato de los seis existentes — contexto, decisión, alternativas descartadas **con su motivo**, consecuencias a favor y en contra.
3. Si la puerta te estorba en algún punto, **no la rodees**: dilo. Que un control de seguridad resulte incómodo es información sobre el diseño, y ADR-0006 se puede revisar. Lo que no se puede es tener una puerta con una excepción no escrita.
4. No hagas commit tú mismo — deja el árbol de trabajo listo y dime qué cambió.

Sé exigente contigo mismo con el mismo rigor que hubo en M1, M2, M3 y en la puerta: no reportes "hecho" hasta haberlo verificado contra datos reales, no sintéticos.
