# ADR-0008 — `multilingual-e5-small` sobre ONNX Runtime

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0003](0003-lenguaje-del-nucleo.md)
- **Contesta a:** los invariantes I7 y F1

## Contexto

M4 mete el primer modelo dentro del sistema. El invariante I7 avisa de lo que está en juego:

> *Los embeddings son el lock-in real, no el LLM.*

Un LLM se cambia editando una llamada. Un índice vectorial de dos años de antigüedad **no se cambia: se tira**, porque los vectores de un modelo no significan nada para otro. La decisión no es qué modelo es mejor hoy; es cuál se puede sostener durante años y cómo se sale de él.

Tres restricciones acotan la elección antes de mirar ninguna tabla de resultados:

1. **El corpus es multilingüe.** Castellano, inglés y código fuente en el mismo índice — `REVISION y PROPUESTA de Mejora al Almacén`, `bindings_main.js` y `PROJECT-MASTER.md`. Un modelo monolingüe inglés queda descartado sin medir.
2. **F1: local-first.** Después de bajar el modelo una vez, todo tiene que funcionar sin conexión. Para siempre y sin excepciones «sólo la primera vez».
3. **CPU.** La máquina tiene una RTX 4050 de 6 GB y **no se usa**. 2.479 fragmentos en CPU son minutos; añadir CUDA es complejidad de despliegue a cambio de nada.

## Decisión

**`intfloat/multilingual-e5-small`, pesos ONNX en `float32` del repositorio oficial, ejecutado con `ort` (ONNX Runtime) y `tokenizers`.**

| | |
|---|---|
| Dimensión | 384 |
| Parámetros | ~118 M (de los cuales ~96 M son la tabla de embeddings de 250.037 entradas) |
| Descarga | 448,5 MB `model.onnx` + 16,3 MB `tokenizer.json` |
| Longitud máxima | 512 tokens |
| Pooling | media enmascarada, luego normalización L2 |
| Prefijos | `query: ` / `passage: ` — **obligatorios y distintos** |

Y tres decisiones de integración que valen tanto como la elección del modelo:

### El rol viaja en el contrato, no en la documentación

E5 exige prefijar la entrada con `query: ` o `passage: `. Sin ellos el modelo **sigue devolviendo vectores de aspecto razonable** y la calidad baja sin que nada falle: no hay excepción, no hay contador, no hay test en rojo. Es la clase de error que sobrevive años.

Por eso `Embedder::embed` toma un `Role` y **no existe un `embed(texts)` a secas**. Un modelo que no necesite prefijos lo ignora; uno que sí, no puede recibirlo mal.

### La descarga es un subcomando, no un efecto secundario

`obrain-embed <db> --descargar-modelo`. Nada más del crate abre un socket, y `E5::load` falla con un error que dice qué ejecutar en vez de ir a buscar los archivos.

### La normalización es parte del contrato

`Embedder::embed` devuelve vectores de norma 1, y `index::insert` **rechaza** los que no lo sean. Con vectores unitarios el coseno *es* el producto escalar; un implementador que devuelva vectores sin normalizar rompería el kNN en silencio.

## Alternativas consideradas

### Modelos

#### A. `paraphrase-multilingual-MiniLM-L12-v2`

Misma dimensión (384) y tamaño parecido. Rechazado por antigüedad: es de 2021 y está entrenado con destilación de paráfrasis, no con el objetivo contrastivo de recuperación con el que se entrenó E5. En las evaluaciones públicas de recuperación multilingüe queda sistemáticamente por debajo. No aporta nada que E5 no dé.

#### B. `bge-m3`

Mejor calidad, y con margen: 1.024 dimensiones, recuperación densa, dispersa y multivector a la vez, 8.192 tokens de contexto. Rechazado **para este hito** por peso: 2,2 GB de pesos y 2,7× el tamaño de cada vector. Con 2.479 fragmentos el índice pasaría de 3,6 MB a 10,2 MB —irrelevante— pero el coste de inferencia se multiplica por un factor que no se ha medido, y M4 tiene que poder decir cuánto cuesta vectorizar el corpus.

Es el candidato obvio para la primera migración I7, y esa migración es precisamente lo que este hito deja construido: dos modelos conviven en `vectors` porque su identidad está en la clave primaria.

#### C. El mismo E5 cuantizado a int8 (~113 MB)

Era la recomendación de peso, y es lo que este proyecto acabará queriendo. Rechazado por dos motivos que se refuerzan: el repositorio oficial **no publica un ONNX cuantizado** —el que existe es una reexportación de terceros—, y la cuantización mueve la calidad justo en el hito cuyo trabajo es medirla. Primero se mide la referencia; cuantizar después es un cambio con un antes y un después que comparar.

#### D. Un modelo tras una API remota

Rechazado por F1 sin más discusión, y por I11: la mitad del trabajo de la puerta de sensibilidad existe para que esto no pase.

### Runtime

#### E. `fastembed-rs`

Envuelve `ort` + `tokenizers` y es notablemente más cómodo. Rechazado porque **descarga los modelos de HuggingFace en tiempo de ejecución**. Se puede envolver en un paso deliberado, pero entonces se está peleando con la librería para recuperar la propiedad que la librería regaló, y la promesa central del producto queda dependiendo de que nadie llame a la API cómoda.

#### F. `candle` (Rust puro)

Tentador y con un argumento fuerte: sin librería nativa, todo compilado dentro del binario, que es literalmente lo que pide §7.2 («binario nativo único: no hay runtime que instalar»). Rechazado para M4 por madurez y por rendimiento en CPU — es donde el maestro decía que había que justificarlo con una medición, y la medición honesta es que **no se ha hecho**: implementar las dos rutas para compararlas era el hito entero.

Se deja anotado como deuda concreta. Si el coste de `ort` (abajo) resulta molesto al empaquetar, `candle` es la salida y la comparación habrá que hacerla entonces, con números.

#### G. GPU (CUDA / DirectML)

Rechazado por lo dicho en el contexto: 2.479 fragmentos en **4,6 minutos** de CPU. Añadir un proveedor de ejecución de GPU es complejidad de despliegue y de soporte a cambio de minutos que sólo se pagan una vez.

## Consecuencias

**A favor**

- **El corpus real se vectoriza en 4,6 minutos** (2.479 fragmentos, 9 fragmentos/s, un solo hilo de control y ONNX Runtime paralelizando dentro). La carga del modelo son 2,5 s.
- **384 dimensiones = 3,6 MB de índice** para todo el corpus. Cabe en caché de CPU, que es lo que hace que el recorrido lineal de [ADR-0010](0010-recorrido-lineal-frente-a-indice-ann.md) sea la respuesta correcta.
- **Multilingüe de verdad.** Una consulta en castellano encuentra documentos en castellano y en inglés; sobre este corpus concreto se comprobó con `universidad`, que devuelve un formulario de matrícula que no contiene la palabra.
- **Sale del modelo sin dolor.** Cambiar a `bge-m3` es escribir un `Embedder` nuevo y re-embeber en paralelo. Ninguna otra parte del sistema sabe qué modelo hay debajo.
- **La red se toca una vez y se ve.** Un subcomando, un mensaje con el origen, y órdenes `curl` equivalentes para quien prefiera bajarlo a mano o llevar el directorio a una máquina sin conexión.

**En contra**

- **ONNX Runtime no es Rust y `ort` sigue en release candidate.** `2.0.0-rc.13`; la 2.0 estable no ha salido. Es una dependencia joven en el camino crítico.
- **El «binario nativo único» de §7.2 ya no es exacto.** `ort` enlaza ONNX Runtime **estáticamente**, así que el `.exe` sí lo lleva dentro; pero la distribución de Windows que descarga trae DirectML, y el binario acaba necesitando `DirectML.dll` al lado. Windows 10 1903+ la trae en `System32`, así que en la práctica funciona — pero es una dependencia externa que antes no había y no estaba prevista.
- **`ort` descarga la librería nativa en tiempo de *compilación*** desde una CDN de terceros (pyke), fuera de crates.io. Es una dependencia de la cadena de suministro que hay que nombrar. Se puede fijar con `ORT_LIB_LOCATION`; no se ha hecho.
- **La descarga del modelo no verifica firmas.** Se apoya en TLS y en que el origen sea el repositorio oficial — lo mismo que cualquier `pip install`, y no suficiente para un sistema que se tome en serio la cadena de suministro. Comprobar el hash publicado contra el archivo bajado es el paso siguiente.
- **512 tokens de contexto.** `paragraph-pack@1` produce fragmentos de 1.200 bytes, que en castellano rondan los 300 tokens, así que el truncado casi nunca se activa. El día que el chunker cambie, los dos números hay que mirarlos juntos.
- **La versión del `ModelId` es de la integración, no del repositorio.** Si `intfloat` republicara los pesos con el mismo nombre, nada lo detectaría. Guardar el hash del `model.onnx` junto a los vectores cerraría el hueco y no se hace.
