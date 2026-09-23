<div align="center">

# 🧠 Orchestrator's Brain

**Tu segundo cerebro. El que no olvida.**

*Un sistema de conocimiento personal que no solo guarda tus archivos —entiende qué significan, cómo se relacionan y dónde encajan en tu vida.*

---

[![Estado](https://img.shields.io/badge/estado-en%20desarrollo%20activo-brightgreen?style=for-the-badge)](#-estado-actual)
[![Licencia](https://img.shields.io/badge/licencia-MIT-blue?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/núcleo-Rust-orange?style=for-the-badge)](#)
[![Local First](https://img.shields.io/badge/tus%20datos-nunca%20salen-purple?style=for-the-badge)](#-privacidad-como-principio)

</div>

---

## 💡 La idea en una frase

> *«¿Dónde estaba el documento que usé para demostrar ingresos durante mi trámite de visa?»*

Esa pregunta no contiene el nombre del archivo, ni su carpeta, ni su fecha. Contiene una **relación** entre cosas que viviste: un documento, un trámite y un propósito. Ningún buscador de archivos puede responderla. **Orchestrator's Brain sí.**

---

## 🤔 El problema

Piensa en todos los archivos que tienes en tu computadora. PDFs de trámites, fotos de viajes, documentos de trabajo, facturas, recibos, notas. Miles de archivos acumulados a lo largo de años.

Ahora piensa en lo que **sabes** sobre esos archivos que ningún sistema conoce:

- Ese PDF era parte de un trámite de visa
- Esas 340 fotos son de un viaje a Barcelona en 2019
- Ese documento Word fue la versión final de un presupuesto que pasó por 4 revisiones
- Ese recibo lo descargaste de una página web específica para justificar un gasto

**Todo ese conocimiento vive solo en tu cabeza.** Y tu cabeza olvida.

Las herramientas actuales te permiten buscar por nombre de archivo, por fecha, quizás por alguna palabra dentro del documento. Pero no pueden responder preguntas sobre el **significado** de tus archivos ni las **relaciones** entre ellos.

Orchestrator's Brain existe para resolver eso.

---

## 🌌 La visión: un mapa de tu conocimiento

Imagina abrir una aplicación y ver algo parecido a una constelación: puntos de luz conectados entre sí, unos más brillantes que otros, algunos agrupados en racimos densos, otros aislados en la periferia.

Cada punto es algo que el sistema conoce: un archivo, una persona, un trámite, un evento. Las conexiones entre ellos son relaciones que el sistema descubrió: este documento pertenece a ese trámite, fue escrito por esta persona, se descargó de aquella página.

- **Las zonas brillantes** son donde tu vida está activa ahora mismo
- **Las zonas tenues** son tu pasado — quieto pero intacto, listo para cuando lo necesites
- **Las conexiones punteadas** son cosas que el sistema *cree* pero no está seguro — y tú puedes confirmar o corregir con un clic

No es un salvapantallas bonito. Es un **mapa funcional de tu conocimiento** que se va haciendo más rico con el tiempo.

---

## ✨ Qué hace (y qué lo hace diferente)

### 🔍 Encuentra lo que no recuerdas que tienes

No necesitas recordar el nombre del archivo. Ni la carpeta. Ni la fecha exacta. Puedes buscar por **significado**: "el documento que usé para demostrar ingresos" es una búsqueda válida.

### 🕸️ Descubre relaciones que tú no le dijiste

El sistema conecta tus archivos automáticamente usando señales que ya existen en tu computadora: metadatos, fechas, contenido, estructura de carpetas, incluso la URL de donde descargaste algo. No inventa — descubre.

### 🧩 Reconstruye procesos completos

Un trámite de visa no es un archivo: es un conjunto de documentos, fechas, personas y pasos. Orchestrator's Brain puede identificar estos **procesos** y reconstruirlos como una historia coherente.

### 🫥 Nunca inventa

Si no sabe algo, dice "no lo sé" y te muestra qué buscó y por qué no encontró nada. Una respuesta inventada es peor que ninguna respuesta.

### ✅ Tú siempre tienes la última palabra

Si el sistema infiere algo incorrecto, lo corriges con un clic. Tu corrección queda **grabada permanentemente** y nunca será sobreescrita. Tu conocimiento manda sobre cualquier inferencia.

---

## 🔒 Privacidad como principio

Esto no es negociable:

| Principio | Lo que significa |
|-----------|-----------------|
| **Local primero** | Tus datos viven en tu computadora. El procesamiento ocurre en tu máquina. |
| **Nada sale sin tu permiso** | Cada archivo tiene un nivel de sensibilidad. Los documentos sensibles nunca se envían a ningún servicio externo. |
| **Cifrado en reposo** | Tu base de conocimiento está cifrada en tu disco (SQLCipher), y la llave la guarda Windows para tu cuenta. Una copia de la base no se puede leer. Te damos una clave de recuperación por si Windows pierde la llave. **No cifra tus archivos originales**: para eso está el Cifrado de dispositivo de Windows. |
| **Un PIN para ver** | Si pones un PIN, lo sensible no se enseña —ni en la vista previa ni en la búsqueda— hasta escribirlo. Lo impone el núcleo, no la ventana. |
| **Sin cuenta, sin nube obligatoria** | Funciona completamente offline. La sincronización entre dispositivos es opcional y cifrada de extremo a extremo. |

Si vas a guardar tu pasaporte, tus contratos y tus documentos médicos en un sistema, ese sistema **no puede tener acceso a tus datos**. Esta es la razón por la que el procesamiento es local — no por dogma, sino por responsabilidad.

---

## 🗺️ Cómo funciona (sin tecnicismos)

Orchestrator's Brain trabaja en capas, como un cerebro real:

### 1. 👀 Observar — *Ver tu disco*
El sistema recorre las carpetas que le indiques y **reconoce** cada archivo — no por su nombre o ubicación (que pueden cambiar), sino por su **contenido**. Si mueves o renombras un archivo, sigue siendo el mismo.

> Inteligencia incluida: detecta archivos que están "en la nube" y no los descarga sin tu permiso. No va a llenar tu disco ni saturar tu internet.

### 2. 📖 Leer — *Entender el contenido*
Extrae el texto de cada documento (PDFs, documentos de Office, notas, código) y lo organiza para poder buscarlo.

> Las **fotos y los PDF escaneados** —la foto de tu TIE, el pasaporte escaneado— se leen con el reconocedor de texto (OCR) que **ya trae Windows**, sin conexión: nada sale de tu computadora.

> Y de ese texto saca **cuándo caduca** cada documento —«VALIDEZ 15 11 2026» en la TIE, «vigencia hasta el 31 de agosto de 2027» en la póliza—: el Inicio te avisa en **«Caduca pronto»** de lo que vence en los próximos 90 días. Cada fecha dice por qué la leyó, y si se equivoca la corriges en un clic; lo que digas tú gana siempre.

### 3. 🔗 Relacionar — *Conectar sin adivinar*
Antes de usar cualquier inteligencia artificial, el sistema ya puede descubrir **miles de relaciones** usando solo señales que existen en tus archivos:

| Lo que encuentra | Cómo lo sabe |
|------------------|--------------|
| Archivos duplicados | Mismo contenido, aunque tengan diferente nombre |
| Versiones de un documento | Nombres similares con marcadores como "v2", "final", "rev" |
| Sesiones de trabajo | Archivos modificados en la misma ventana de tiempo |
| Origen web | La URL de donde descargaste un archivo (sí, Windows guarda eso) |
| Estructura lógica | Las carpetas que tú mismo organizaste son una señal muy fuerte |

### 4. 🧠 Entender — *Buscar por significado*
Con un modelo de inteligencia artificial **que corre en tu máquina** —se descarga una vez y después funciona sin conexión—, el sistema busca por significado: encuentra documentos relevantes aunque no contengan las palabras exactas que buscaste.

> Ejemplo real de este disco: buscar **"universidad"** devuelve un formulario de matrícula que **no contiene esa palabra** en ningún sitio. Dice "Titulación", "Centro Docente", "Año académico". La búsqueda por texto no puede encontrarlo; ésta sí.
>
> Y lo que marcaste como privado **no entra** en el índice de significado. Eso no es una opción de configuración: es una comprobación que el sistema aplica y que tiene tests.
>
> Puedes marcar **una carpeta entera** —por ejemplo, la de tus documentos de identidad— y todo lo que tenga dentro será sensible, también lo que metas después: la foto de tu TIE no necesita tener texto ni un nombre que la delate.

### 5. 🌌 Visualizar — *El mapa*
Todo ese conocimiento cobra vida en una visualización interactiva — tu galaxia personal de conocimiento, donde puedes explorar, navegar y descubrir conexiones que habías olvidado.

Y cuando llegas a un archivo, **lo ves**: el texto que ya se había extraído —con el párrafo que explica *por qué* apareció, resaltado—, o la imagen si es una foto. Al lado, lo que el sistema sabe de él separado en dos bloques que no se pueden confundir: lo que es un hecho y lo que es una sospecha, cada sospecha con su «¿por qué?» y con un botón para corregirla.

> Lo que está en tu nube **no se toca**. Un archivo de OneDrive que aún no se ha descargado se marca con ☁ y su previsualización no se genera, porque generarla lo bajaría a tu disco sin que lo hubieras pedido. Si quieres abrirlo, el botón te dice con todas las letras que hacerlo lo descargará.

---

## 📈 Estado actual

El proyecto está en desarrollo activo. Cada hito se valida contra la pregunta del inicio: *"¿Puede responder dónde estaba el documento de la visa?"*

> **2026-09-22 — replanteamiento.** El objetivo inmediato deja de ser *entender todo el disco* y pasa a ser **una bóveda de documentos personales que eliges tú**: pasaporte, seguros, matrícula, contratos. Dónde está todo y cómo usarlo hoy: [`docs/ESTADO.md`](docs/ESTADO.md).

| Hito | Descripción | Estado |
|------|-------------|--------|
| **M0** — Cimientos | Modelo de datos y almacenamiento | ✅ Completo |
| **M1** — Ver el disco | Escaneo inteligente de archivos | ✅ Completo |
| **M2** — Leer el disco | Extracción de texto y búsqueda | ✅ Completo |
| **M3** — Relacionar | Conexiones automáticas sin IA | ✅ Completo |
| **Privacidad** — La puerta | Clasificación de sensibilidad y control de egreso | ✅ Completo |
| **M4** — Entender | Búsqueda por significado con IA local | ✅ Completo |
| **M5a** — La barra | Buscar desde una barra, con el mapa vivo detrás | ✅ Completo |
| **M5b** — Llegar y ver | Explorador por columnas y panel de entidad con previsualización | ✅ Completo |
| **M5d** — Manejarlo | Cámara del mapa —zoom y arrastre— y saber qué nodo vas a abrir antes de abrirlo | ✅ Completo |
| **Fase 1** — Tu bóveda | Bibliotecas: el explorador empieza en tu carpeta, nombres limpios, `obrain indexar`, la app se pone al día sola | ✅ Completo |
| **M5c** — Enterarse | Lo nuevo aparece sin lanzar nada | ◐ Al abrir la app y con «actualizar»; falta vigilar la carpeta en vivo |
| **Fase 2** — La interfaz nueva | Svelte 5: escritorio tipo sistema operativo con vista previa, buscador `Ctrl K`, la galaxia como vista «Mapa» enlazada, aspecto de noche y de día | ✅ Completo |
| **Fase 3** — Documentos personales | Sensibilidad por carpeta ✅ · cifrado en reposo y PIN ✅ · OCR ✅ · vencimientos ✅ · vigilar la carpeta en vivo | ◐ En curso |
| **M6** — Construir conocimiento | Inferencia de procesos y enriquecimiento | ⏳ Planificado |
| **M7** — La pregunta | Responder la pregunta de la visa | 🎯 Meta final |

### Números reales del último escaneo

Medidos el **2026-08-17** sobre el perfil real del usuario —OneDrive, descargas, capturas y documentos de vida personal—, con las herramientas del propio proyecto:

- **3,821** archivos recorridos y **46 directorios podados** en **0.66 segundos**
- **1,130** archivos en el índice, **1,429 entidades**, **16,424 aserciones** en el log
- **2,410 relaciones** vivas descubiertas — sin una sola llamada a inteligencia artificial
- **4,533 fragmentos** de texto indexados y buscables
- **4,501 vectores** de significado (los otros **32 son privados** y se quedan fuera a propósito)
- Búsqueda por significado sobre los 4,501 vectores en **8.21 ms** — el recorrido lineal aguanta hasta **~27,400** antes de necesitar otra cosa
- **0 archivos** descargados innecesariamente de la nube

> El 86 % de lo indexable del perfil no era del usuario: eran **22,000 archivos de estado de herramientas** —`.codex`, `.vscode`, `.gemini`, `.m2`— dentro de la propia carpeta personal. La corrección no fue alargar la lista de exclusiones, que vuelve a romperse con la herramienta del mes que viene, sino añadir una señal: *un directorio con punto que cuelga de la raíz del perfil es estado de un programa, no contenido tuyo*. Un buscador que compite contra la basura del disco no es un buscador.

---

## 🌱 Cómo evoluciona con el tiempo

Esta es la parte que más importa. Orchestrator's Brain no es una herramienta que instalas y funciona igual para siempre. Es un sistema que **crece contigo**:

### Semana 1 — *El archipiélago*
> Islas de archivos agrupados. Ya encuentras duplicados, versiones y sesiones de trabajo. Ya sirve.

### Meses 1 a 3 — *Los puentes*
> La inteligencia artificial empieza a conectar las islas. Aparecen los primeros procesos reconstruidos. El momento *"¿cómo supo eso?"*

### Año 1 — *El relieve*
> Se forma una topografía estable. Las zonas activas de tu vida brillan; el pasado está quieto pero accesible. Empiezas a redescubrir cosas que habías olvidado.

### Año 3+ — *La memoria*
> Más historia que presente. Patrones que se repiten, hilos que quedaron abiertos, contradicciones a lo largo de los años. Ya no es una herramienta — es una **biografía viva**.

---

## 🏗️ Arquitectura (para los curiosos)

<details>
<summary>Clic para expandir los detalles técnicos</summary>

### Stack

- **Núcleo:** Rust — rendimiento predecible, memoria controlada, sin pausa de recolector de basura. Ideal para un proceso que corre todo el día en segundo plano.
- **Persistencia:** SQLite — un solo archivo, cero instalación. Todo índice es una proyección reconstruible desde un log maestro.
- **Interfaz:** Tauri v2 + Svelte 5 + TypeScript — ~10 MB contra ~150 MB de Electron, usa el webview del sistema. Tipografías e iconos van dentro de la app: funciona sin conexión.
- **Visualización:** PixiJS (WebGL2) — renderizado verificado a 0.6 ms para 500 nodos en GPU integrada Intel.

### Modelo de datos

La unidad fundamental no es el archivo ni el nodo: es la **aserción** — *alguien afirmó algo, en un momento, con una confianza, basándose en una evidencia*. Los nodos y las relaciones son vistas derivadas del log de aserciones.

Esto hace que la auditoría, el deshacer, la sincronización y la detección de contradicciones salgan gratis del modelo, en vez de ser features que se parchean después.

### Decisiones de diseño documentadas

El repositorio contiene un registro formal de decisiones arquitectónicas (ADRs) en [`docs/adr/`](docs/adr/) que explican el *porqué* de cada elección fundamental.

</details>

---

## 📂 Estructura del repositorio

```
Orchestrator's-Brain/
├── core/                          # Núcleo del sistema (Rust)
│   ├── obrain-keys/               #   La llave de la base (DPAPI) y la clave de recuperación
│   ├── obrain-core/               #   Modelo de datos y almacenamiento (cifrado con SQLCipher)
│   ├── obrain-scan/               #   Descubrimiento e identidad de archivos
│   ├── obrain-ocr/                #   Leer fotos y escaneos con el OCR de Windows
│   ├── obrain-extract/            #   Extracción de texto y búsqueda léxica
│   ├── obrain-link/               #   Relaciones automáticas entre archivos
│   ├── obrain-guard/              #   Sensibilidad y control de egreso
│   ├── obrain-expiry/             #   Vencimientos: la fecha de caducidad, leída del texto
│   ├── obrain-embed/              #   Embeddings locales y búsqueda semántica
│   └── obrain-api/                #   La superficie: buscar, mirar y recorrer
├── ui/                            # Aplicación de escritorio (Tauri + Svelte 5)
│   ├── src/
│   │   ├── vistas/                #   Inicio, Carpeta, Mapa y Sensibles
│   │   ├── componentes/           #   Barra lateral, cabecera, buscador, vista previa…
│   │   ├── lib/                   #   Estado de la app, nombres legibles, el mapa (PixiJS)
│   │   └── estilos/               #   El tema: noche y día
│   └── src-tauri/                 #   El puente al núcleo
├── docs/                          # Documentación de diseño
│   ├── ESTADO.md                  #   Empieza aquí: dónde estamos y cómo usarlo
│   ├── PROJECT-MASTER.md          #   Documento maestro del proyecto
│   ├── EXPERIENCE.md              #   Diseño de experiencia y lenguaje visual
│   ├── adr/                       #   Registro de decisiones arquitectónicas
│   └── archivo/                   #   Bitácora de agosto y prompts de diseño
└── prototypes/                    # Prototipos y pruebas de concepto
    └── 01-graph-render/           #   Benchmark de renderizado WebGL
```

---

## 🤝 Sobre el proyecto

Orchestrator's Brain es un proyecto personal en desarrollo activo. Nació de una frustración concreta: tener miles de archivos acumulados durante años y no poder preguntarle a la computadora nada que no sea "¿cómo se llama?" o "¿cuándo lo modificaste?".

La ambición es construir algo que funcione como un segundo cerebro real: que recuerde lo que tú olvidaste, que vea conexiones que tú no ves, que organice tu conocimiento sin pedirte que lo organices tú — y que lo haga sin que tus datos salgan de tu máquina.

No es un proyecto terminado. Es un proyecto que se va a ir perfeccionando con el tiempo, hito por hito, validando cada paso contra una pregunta real: *¿puede encontrar aquel documento de la visa?*

Y el día que pueda, la arquitectura habrá demostrado que funciona.

---

<div align="center">

*Construido con la convicción de que tu computadora debería conocerte mejor — sin necesidad de contarle nada a nadie más.*

**[🧭 Estado](docs/ESTADO.md)** · **[📖 Documento Maestro](docs/PROJECT-MASTER.md)** · **[🎨 Diseño de Experiencia](docs/EXPERIENCE.md)** · **[📋 Decisiones Arquitectónicas](docs/adr/)**

</div>
