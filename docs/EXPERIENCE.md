# Orchestrator's Brain — Diseño de Experiencia

**Estado:** Fase 3 · **Última revisión:** 2026-08-10 · **Documento vivo**

Complementa a [PROJECT-MASTER.md](PROJECT-MASTER.md). Todo lo de aquí está restringido por los invariantes de §2 del maestro — en particular I2 (el grafo es una lente), I9 (nunca respuesta sin procedencia) e I10 (la animación jamás en el camino crítico).

---

## 1. La ley de diseño

Todo el lenguaje visual se deriva de una sola regla:

> ## La luz es información, no decoración.

Si el brillo es decorativo, la interfaz es ruido bonito y el usuario aprende a ignorarla. Si el brillo **significa** algo, el usuario aprende a leer el mapa sin leyenda — y en ese momento deja de parecer un salvapantallas y empieza a parecer un cerebro.

De ahí sale toda la codificación visual, y no se admite ningún efecto que no case con ella:

| Canal visual | Codifica | Por qué es la elección correcta |
|--------------|----------|--------------------------------|
| **Luminosidad** | Relevancia respecto al foco actual | Es el canal más preatentivo. Se lee sin mirar. |
| **Saturación** | Confianza de la aserción | Una inferencia dudosa sale literalmente **pálida**. La intuición es inmediata y no hay que explicarla. |
| **Trazo de arista** | Método: sólida = determinista, punteada = inferida | Distinguir hecho de inferencia de un vistazo (I9). |
| **Opacidad de arista** | Confianza de esa relación concreta | |
| **Tamaño de nodo** | Densidad de conexiones | Nunca "importancia" arbitraria: eso sería una opinión disfrazada de dato. |
| **Movimiento residual** | Recencia | Lo tocado hace poco tiene un latido casi imperceptible. Lo antiguo está quieto. |

Ese último canal es el que hace real la palabra "respirar" de la visión original, y **con función**: el mapa te muestra dónde está viva tu vida ahora mismo. Las zonas antiguas están en calma. Las activas respiran. No es un efecto: es un dato.

## 2. Cómo se siente

Oscuro, profundo y silencioso. La referencia no es el neón ni el cyberpunk: es **un observatorio de noche**, o agua profunda. Casi todo está tenue y quieto. Mirar algo lo enciende.

Tres compromisos que definen la sensación más que cualquier paleta:

**Silencio por defecto.** La aplicación no reclama atención. No hay insignias, ni contadores, ni notificaciones de "tienes 14 cosas sin revisar". Un segundo cerebro que te da tareas no es un cerebro, es un jefe.

**La latencia es el lujo.** Abrir en 40 ms se siente caro. Abrir en 400 ms se siente barato aunque tenga shaders. Este es el punto donde la arquitectura local-first se convierte en experiencia: no hay red en el camino crítico, así que el objetivo es real y no aspiracional.

**Modo reposo.** Con la ventana sin foco o el usuario inactivo, el bucle de render cae casi a cero. Una galaxia WebGL a 60 fps constantes calienta el portátil y se come la batería, y una herramienta de conocimiento que hay que cerrar para que dure la tarde ha fracasado como producto. Esto es un compromiso de diseño, no una optimización opcional.

---

## 3. Vocabulario de animación

Seis animaciones. Sólo seis. Un vocabulario corto y consistente se lee como intención; uno largo se lee como decoración.

| # | Nombre | Qué hace | Duración |
|---|--------|----------|----------|
| 1 | **Asentamiento** | Los nodos nunca *aparecen*: llegan desde su posición previa y se asientan con amortiguación. | ~400 ms |
| 2 | **Foco** | Cambiar de foco no reconstruye el grafo: lo **repondera**. La cámara se desplaza, lo irrelevante se atenúa y desatura. Nada se teletransporta. | ~300 ms |
| 3 | **Expansión** | Al expandir un nodo, sus vecinos crecen hacia fuera **escalonados 15–25 ms**. Se lee como una onda, no como un estallido. | ~500 ms |
| 4 | **Latido** | Amplitud mínima, periodo largo (3–5 s), desincronizado por nodo, amplitud ∝ recencia. | continuo |
| 5 | **Trazo** | Al explicar una respuesta, el camino se **dibuja solo**: pregunta → Process → intervalo → documentos. | ~900 ms |
| 6 | **Sedimentación** | El conocimiento nuevo creado en segundo plano aparece con un fundido lento en la periferia. | ~2 s |

**El escalonado del #3 es el truco más barato que existe para que una interfaz se sienta cara.** Un stagger de 20 ms convierte un cambio de estado en un gesto.

**El #5 es la animación que vende el producto.** No es decoración: **es la explicación**. Cuando el sistema dice "está aquí", dibujar el camino que siguió para saberlo es lo que convierte la procedencia (I9) en algo visceral en vez de en una nota al pie.

**El #4 tiene que ser casi subliminal.** Si conscientemente ves los nodos latir, está demasiado fuerte. Debe notarse al quitar la vista, no al mirarlo.

### Disciplina, no negociable

- El contenido se pinta en el **frame 1**. La ambientación se anima alrededor (I10).
- **Cualquier animación se interrumpe al instante** con input del usuario. Jamás una transición que bloquee.
- `prefers-reduced-motion` colapsa las seis a fundidos de opacidad. La app sigue siendo completamente utilizable.
- **El mapa nunca se reorganiza solo mientras el usuario está mirando.** Los cambios de segundo plano se encolan y se aplican en el siguiente cambio de foco. Un mapa que se mueve solo se siente embrujado, no vivo.

---

## 4. Navegación

### 4.1 La escalera

Un único punto de entrada, y una escalera de escalones opcionales:

```
tecla global → barra → resultado → previsualización → nodo en el mapa → región → deriva
     ↑                    ↑                                                      ↑
  40 veces/día      donde paras                                        donde te pierdes
                     casi siempre                                        a propósito
```

Cada escalón es **una tecla**. Y se puede parar en cualquiera. La mayoría de los días paras en el segundo — y eso es el diseño funcionando, no fallando.

La escalera cubre el camino de quien **busca**. Atravesándola hay un segundo eje, para quien ya sabe qué quiere y sólo necesita bajar hasta ello: el **conmutador de vista** (§4.7), que también es una tecla y está disponible en cualquier escalón.

### 4.2 La barra flota sobre el mapa, no lo sustituye

Aquí se resuelve la tensión que planteé en Fase 1 (la galaxia como obstáculo hermoso).

La barra de comandos **se dibuja encima de un mapa vivo y atenuado**. Mientras escribes, el mapa del fondo reacciona: las regiones que coinciden se encienden débilmente, el resto se apaga un poco más.

Ves tu conocimiento responder **por el rabillo del ojo, antes de decidir mirarlo.**

Eso hace tres cosas a la vez: te enseña que el mapa existe sin exigirte usarlo, te da una pista periférica de dónde está la respuesta, y convierte la transición entre recuperar y comprender en un gradiente en vez de en un salto entre pantallas.

### 4.3 Los cuatro modos del mapa

| Modo | Para qué | Presupuesto |
|------|----------|-------------|
| **Foco** | Una entidad al centro, radio 2 | ~300 nodos |
| **Proceso** | Un `Process` y todos sus artefactos — *la vista de la visa* | ~150 nodos |
| **Tiempo** | Eje temporal explícito. **El único sitio donde el 3D se gana su lugar**, porque la profundidad significa algo. | variable |
| **Deriva** | Sin consulta. Vagar lento por el grafo. | ~200 nodos |

**Deriva es el único modo sin objetivo, y es el que cumple la promesa más difícil de la visión**: ayudarte a encontrar algo cuando ya no recuerdas que existe. No se puede buscar lo que has olvidado. Sólo se puede tropezar con ello.

### 4.4 El panel de entidad

Al seleccionar un nodo se abre un panel lateral que **no destruye el mapa** (nunca un modal: perder el contexto espacial es perder el producto).

> **Alcance de esta prohibición** ([ADR-0014](adr/0014-dos-vistas-un-solo-estado.md)). Lo que se prohíbe es que **tocar un nodo** haga aparecer una superposición que tape el mapa: eso es contexto que el sistema te quita sin pedirlo. No se prohíbe una vista que el usuario **invoca a propósito** — ver §4.7. Se ven parecidas en pantalla y son opuestas para quien las usa. La regla, dicha con precisión: **nada tapa el mapa como consecuencia de tocar algo del mapa.**

Contiene, en este orden:

1. **Qué es** — facetas, no un "tipo" rígido.
2. **Observado** — bloque sólido. Hechos deterministas. Sin adornos.
3. **Inferido** — bloque desaturado, visualmente distinto. Cada línea con un **"¿por qué?"** que resalta el fragmento exacto del documento origen del que salió.
4. **Corrección** — en todo lo inferido, siempre.

### 4.5 La corrección es de primera clase

Consecuencia directa del invariante I5, y probablemente el detalle de UX que más decide si confías en este sistema con tu pasaporte.

Corregir cuesta **un clic**. Y la interfaz tiene que confirmar de forma **visible y permanente** que la corrección se ha fijado: la aserción pasa de pálida (inferida) a sólida (usuario), delante de ti, y no vuelve a cambiar nunca.

El usuario tiene que **ver físicamente su autoridad sobre el sistema.** Si un reescaneo revierte alguna vez una corrección, el producto está muerto — y por eso esto es arquitectura (§3.1 del maestro), no interfaz.

### 4.6 "No lo sé" es una pantalla diseñada, no un error

Cuando no hay respuesta, el sistema no puede quedarse en blanco ni inventar (I9). Muestra **qué miró**: el rango temporal que consideró, las entidades que resolvió, los candidatos que descartó y por qué.

Un "no lo sé" que enseña su razonamiento genera más confianza que un acierto sin explicación.

### 4.7 El conmutador: galaxia y explorador

Decidido en [ADR-0014](adr/0014-dos-vistas-un-solo-estado.md). Existe porque la escalera de §4.1 sirve al que busca, y hay un segundo usuario que es el mismo en otro momento: el que **ya sabe qué quiere** y sólo necesita bajar hasta ello.

Son dos intenciones distintas, no dos estéticas:

| Intención | Vista |
|---|---|
| «no sé qué busco, enséñame el paisaje» | **Galaxia** |
| «sé exactamente qué quiero, déjame bajar por la jerarquía» | **Explorador** |

Forzar una sola metáfora sobre las dos es cómo un usuario acaba peleándose con su propia herramienta.

> **Actualizado el 2026-09-22 — [ADR-0019](adr/0019-el-explorador-es-el-escritorio.md).**
> Con la bóveda personal el orden se invirtió: **el explorador es el escritorio**
> donde abre la aplicación, y la galaxia es la vista «Mapa», que se invoca. Lo
> que sigue se conserva como se escribió; la continuidad de estado en los dos
> sentidos y «enseña lo que un gestor de archivos no puede» siguen en pie.

**Cómo se comporta**

- **La galaxia es el estado base.** El explorador se pide siempre: un conmutador visible en un sitio fijo, y un atajo de teclado. **Nunca aparece solo.** En el momento en que apareciera por tocar algo, sería el modal que §4.4 prohíbe.
- **Al entrar, la galaxia se congela y se difumina detrás.** Un fotograma, desenfocado, con el bucle de render parado. Es una fotografía tras un cristal, no una escena viva — se ve igual y no cuesta batería (§2, modo reposo).
- **El conmutador cambia la lente, no el destino.** Si estabas mirando `Trámites/Visa` en la galaxia, el explorador abre **ahí**, con esa ruta ya en las migas de pan. Al volver, la cámara queda donde el explorador te dejó. Esto es requisito, no acabado: una vista que te devuelve al inicio no es una vista, es otra aplicación.
- **Se navega el grafo, no el disco.** Se baja por carpetas, y también por *entidad → dónde aparece → documentos que la mencionan*. Ésta es la diferencia que sostiene el proyecto entero: si el explorador sólo enseña carpetas, es el explorador del sistema con pasos de más.
- **Enseña lo que un gestor de archivos no puede.** Las facetas de §4.4, el bloque inferido con su «¿por qué?», la corrección de §4.5 y los puentes entre regiones. La ley de §1 sigue vigente aquí: en las filas del explorador, la saturación sigue significando confianza.

**Lo que no entra en la primera versión:** arrastrar y redimensionar libremente. Se entrega centrado, grande, con tamaños preestablecidos. El valor está en poder llegar a cualquier nodo sin escribir; mover la ventana es acabado y cuesta un tiempo desproporcionado.

**El riesgo, dicho de frente:** si el explorador resulta ser lo cómodo y la galaxia lo bonito, el usuario vivirá en el explorador y esto habrá sido un gestor de archivos con salvapantallas — el fallo que [ADR-0013](adr/0013-el-mapa-es-la-pantalla.md) ya se había señalado a sí mismo. La respuesta no es mutilar el explorador: es que la galaxia se gane su uso, y observar honestamente cuál se abre. Si a los tres meses la galaxia no se abre, el problema es la galaxia.

---

## 5. El arranque en frío

El riesgo R1/R2 de Fase 1. Es donde mueren los productos de este tipo, así que necesita una respuesta real.

**La respuesta equivocada:** enseñar una galaxia vacía e invitar al usuario a añadir cosas.

**La respuesta:** el primer arranque no es un formulario. Es una función.

1. El usuario elige carpetas. **No se le pregunta nada más.**
2. El mapa **no está vacío**: se construye en directo, delante de él, durante el escaneo. Los nodos van asentándose según se descubren.
3. Las aristas deterministas se forman a la vista. Emergen clústeres. El usuario ve su propia vida autoorganizarse durante 90 segundos.
4. **La primera recompensa cae dentro de esa misma sesión**: duplicados encontrados, "estas 340 fotos son de un viaje en 2019", "este documento existe en 4 versiones y ésta es la última".

El escaneo **es** el onboarding. Nunca hay un estado vacío porque nunca hay un estado quieto: hay un estado creciendo, que es la sensación exactamente opuesta.

Y esto sólo es posible porque M1–M3 no necesitan IA y son rápidos. **La arquitectura es la que hace viable el onboarding.**

---

## 6. Cómo evoluciona el mapa con los años

| Era | Forma | Qué entrega | Qué siente el usuario |
|-----|-------|-------------|----------------------|
| **Semana 1** — Archipiélago | Clústeres densos, casi nada entre ellos | Búsqueda que funciona, duplicados, versiones | "Esto ya me sirve" |
| **Mes 1–3** — Puentes | La inferencia conecta islas. Aparecen los primeros `Process` | El momento *"¿cómo ha sabido eso?"* | Sorpresa |
| **Año 1** — Relieve | Topografía permanente: regiones densas y brillantes (vida activa), regiones tenues y quietas (el pasado, intacto) | Deriva empieza a devolver cosas olvidadas | Reconocimiento |
| **Año 3+** — Memoria | Más historia que presente | Contradicciones a lo largo de años, hilos abiertos hace 14 meses, patrones que vuelven cada marzo | Biografía |

Es honesto que la Semana 1 sea un archipiélago: el sistema **no miente sobre conexiones que aún no ha encontrado**. Un grafo denso el primer día sólo se consigue inventando aristas, y ése es exactamente el fallo que mata a este tipo de producto (invariante I3).

### Dos cosas hay que diseñarlas **ahora** para sobrevivir a ese arco

**1. Las posiciones persisten.** Un nodo cuyo sitio aprendiste en el año 1 tiene que estar aproximadamente en el mismo sitio en el año 3. Si el layout se recalcula globalmente, el mapa es inútil como memoria. Por eso el layout estable es arquitectura, no estética.

**2. Agregación semántica por escala.** Al crecer la densidad, el presupuesto de nodos **no crece: crece la agregación.** A poco zoom no ves 40.000 nodos: ves 30 regiones con nombre. Te acercas y una región se despliega. Es *level of detail* aplicado al conocimiento, y es lo único que hace el año 3 navegable.

---

## 7. Lo que deliberadamente no hacemos

| No | Por qué |
|----|---------|
| Dashboard de métricas al abrir | Nadie quiere KPIs sobre su propia vida a las 9 de la mañana |
| Gamificación, rachas, logros | Convierte un cerebro en una app de fitness |
| Notificaciones proactivas en v1 | El silencio es la característica. Se gana el derecho a interrumpir, no se presupone |
| Reorganizar el mapa mientras miras | Se siente embrujado |
| La galaxia como pantalla de carga | Disfrazar latencia de espectáculo es la mentira que delata a un producto |
| 3D en el grafo de conceptos | Oclusión y ambigüedad de profundidad. Sólo en el modo Tiempo, donde el eje significa algo |

---

## 8. Qué hay que prototipar antes de creerse nada de esto

En orden, y con criterio de fallo explícito:

1. **500 nodos con las seis animaciones a 60 fps en un portátil normal** — y medir consumo de batería. Si no se sostiene, todo el capítulo 3 se recorta.

   > **Resultado (2026-08-07) — [`prototypes/01-graph-render`](../prototypes/01-graph-render/index.html)**
   >
   > GPU **Intel UHD integrada**, viewport 1600×900, WebGL2 crudo, 2 draw calls instanciadas.
   >
   > | Escena | Aristas | Render p50 | Render p95 |
   > |---|---|---|---|
   > | 500 nodos | 1.028 | **0,6 ms** | 0,8 ms |
   > | 2.000 nodos | 4.093 | 1,2 ms | 1,5 ms |
   > | 4.000 nodos | 8.144 | 3,2 ms | 3,6 ms |
   >
   > Empaquetado de instancias en CPU: 0,018 ms. Presupuesto de 60 fps: 16,67 ms.
   >
   > **Veredicto: pasa con un margen enorme.** El render no es el cuello de botella, ni de lejos.
   >
   > **Consecuencia que cambia un razonamiento previo:** el presupuesto de ~500 nodos del invariante I2
   > **no está justificado por rendimiento**. Está justificado *sólo* por legibilidad. La restricción se
   > mantiene, pero por el motivo correcto — y hay margen de sobra para efectos más ricos de los previstos.
   >
   > **Lo que esta medición NO cubre, y sigue pendiente:**
   > - Compositing, paint del navegador y comportamiento real de vsync (se midió con `readPixels`
   >   forzando sincronía, no con el bucle de `requestAnimationFrame`).
   > - **Consumo de batería y térmico** — parte del criterio original, aún sin medir.
   > - La pregunta subjetiva de si *se ve* como la visión. No verificada.
2. **Estabilidad del layout** — misma consulta dos veces, mismas posiciones. Si los nodos saltan, el capítulo 6 no existe.
3. **El escaneo en directo del §5** — con una carpeta real de 50.000 archivos. Es el momento que decide si alguien sigue usando la app el día 2.
4. **La animación de Trazo sobre la pregunta de la visa** — es la demo del producto. Si no emociona, hay que rediseñar la explicación, no la animación.
