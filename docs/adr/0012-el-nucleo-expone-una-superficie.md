# ADR-0012 — El núcleo expone una superficie; la aplicación de escritorio es un cliente

- **Estado:** Aceptada
- **Fecha:** 2026-08-08

## Contexto

Hasta M4 el patrón fue siempre el mismo y funcionó: cada hito es una librería con un CLI encima. El CLI abre la base, carga lo que necesite, hace una cosa y muere.

M5 es el primer hito con una interfaz, y ahí el patrón deja de valer por tres motivos concretos, ninguno de comodidad:

1. **Nadie es dueño del estado caro.** Cargar `multilingual-e5-small` cuesta **2,50 s** (informe de M4). En un proceso que muere da igual. En una barra que se abre cuarenta veces al día es la diferencia entre un producto y una demo, y `EXPERIENCE.md` §2 lo eleva a compromiso: *«abrir en 40 ms se siente caro; abrir en 400 ms se siente barato aunque tenga shaders»*.

2. **La firma de M4 es la correcta para medir y la equivocada para dibujar.** `obrain_embed::search::hybrid` devuelve **tres** listas —fusionada, BM25 y kNN— y su propio comentario dice por qué: *«el informe tiene que poder enseñar las tres listas lado a lado»*. Una interfaz que la llame ignora dos y recompone la forma que necesita.

3. **Del grafo sólo había escritura.** `obrain-link` lleva desde M3 produciendo aristas que no lee nadie: `--sample` las enseña de doce en doce y para auditar. La lente del invariante I2 —foco, radio y presupuesto duro— no existía en ninguna parte.

La pregunta de fondo no es dónde poner tres funciones. Es que **el contestador de M7 necesita exactamente las mismas**: resolver una consulta, recorrer el grafo acotado y mirar una entidad con su procedencia.

## Decisión

Un crate nuevo, [`core/obrain-api`](../../core/obrain-api), con un tipo `Brain` que es dueño de la base y del modelo, y **cuatro operaciones**: `search`, `lens`, `entity`, `correct`.

La aplicación de escritorio es un cliente de esa superficie. Sus comandos de Tauri son transporte: coger el cerrojo, llamar y aplastar el error a texto.

Y una consecuencia que se sigue de ahí y conviene escribir aparte: **los canales visuales de `EXPERIENCE.md` §1 se resuelven en el núcleo.** `distance`, `degree`, `method` y `confidence` viajan calculados dentro de cada nodo y cada arista, porque son semántica del conocimiento y no decisiones de dibujo. Una interfaz que dedujera por su cuenta qué es relevante sería una segunda definición de relevancia, y el día que las dos discreparan el mapa mentiría sin que nadie lo notara.

## Alternativas consideradas

### A. Dentro del backend de Tauri (`ui/src-tauri/src/`)

Es la opción por defecto de cualquier plantilla y la que menos archivos crea. Rechazada por dos motivos independientes:

- **M7 tendría que reescribirla.** El contestador no vive en una ventana, y la única forma de reaprovechar la lente sería que el agente dependiera del crate de la aplicación de escritorio.
- **No se puede probar sin abrir una ventana.** Los 22 tests de `obrain-api` corren en memoria en 0,12 s. Dentro de Tauri serían tests de integración con un webview, o no serían.

### B. Ampliar cada crate con sus propias funciones de cara a la interfaz

Rechazada porque **la lente no tiene casa entre los crates existentes**. Necesita el log (`core`), el nombre legible de un nodo (`link`), el nivel de sensibilidad (`guard`) y, para el sustrato, la búsqueda (`extract` + `embed`). Meterla en cualquiera de ellos obliga a que ese crate dependa de los demás por razones de presentación, que es justo la dependencia que la separación por hitos evitó durante cuatro hitos.

### C. Un servidor HTTP local al que hable la interfaz

Rechazada. Añade un puerto que abrir, un problema de autenticación que no existía y una frontera de serialización dentro de la misma máquina a cambio de nada. F1 dice local-first, no local-server. Y dejaría literalmente falsa la frase de §2 sobre que no hay red en el camino crítico.

## Consecuencias

**A favor**

- El contestador de M7 hereda la superficie en vez de reconstruirla.
- La lente es comprobable sin ventana, y sus tests son los del hito.
- El modelo se carga **fuera del cerrojo** (`load_model` + `set_embedder`): la ventana abre con la búsqueda léxica ya funcionando y la mitad semántica se enciende sola cuando está lista. La degradación es una característica declarada — `Answer::semantic` lo dice — y no un plan B silencioso.
- Los canales visuales se deciden una vez.

**En contra**

- Un crate más, y uno que por definición depende de casi todos los demás.
- **Los tipos de salida llevan `serde`**, así que el transporte se filtra un poco en el dominio. Aceptado a conciencia: la alternativa era una capa de DTOs en el backend de Tauri que sería, campo por campo, estos mismos tipos otra vez — exactamente la duplicación que este ADR existe para evitar.
- La superficie es un sitio cómodo donde meter cosas. Cuatro operaciones es un número que hay que defender activamente.
