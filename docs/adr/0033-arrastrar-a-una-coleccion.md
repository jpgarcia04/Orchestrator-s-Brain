# ADR-0033 — Arrastrar a una colección

- **Estado:** Aceptada
- **Fecha:** 2026-09-25
- **Completa:** [ADR-0031](0031-tus-colecciones.md) (la alternativa E, «arrastrar documentos a la colección», que quedó para después)
- **Toca:** [ADR-0019](0019-el-explorador-es-el-escritorio.md) (el explorador como escritorio)

## Contexto

Con ADR-0031 un documento entra en una colección desde su vista previa
(«Colecciones») o eligiendo varios con Ctrl+clic y la barra de abajo. Arrastrar
se dejó fuera por una razón técnica: en Windows, Tauri se queda con el arrastre
(`dragDropEnabled`) para cuando sueltas archivos de fuera, y el arrastre del
navegador (HTML5 *drag and drop*) no llega a la página. Usarlo obligaba a
apagar eso, y sólo se podía comprobar en la laptop del usuario.

El 2026-09-25 el usuario lo pidió desde la nube. Se le enseñó con su coste y
eligió, en dos tandas cortas, antes de escribir código:

| Pregunta | Eligió |
|---|---|
| ¿Desde dónde se arrastra? | **Desde una carpeta y desde una colección**, donde ya se eligen varios. Si arrastras uno de los elegidos, van todos |
| ¿Dónde se suelta? | **En cada colección de la barra lateral**, y en **«Nueva colección»**, que sale en la barra mientras arrastras: al soltar escribes el nombre y se crea con esos documentos |
| De una colección a otra, ¿qué pasa? | **Se mueve; con Ctrl pulsado se queda en las dos**, como en el Explorador de Windows |

Al contestar la tercera preguntó lo importante: *«¿se mueve también en mi
explorador de Windows?»*. **No.** Una colección es como una playlist: el
documento sigue en su carpeta, en Windows y en la app, y moverlo de una
colección a otra sólo cambia en qué playlist está. obrain no mueve, ni
renombra, ni borra archivos (solo-lectura); las carpetas de «Tu bóveda» son
un espejo de las de Windows, así que ahí nunca hay dos organizaciones
distintas. Sacar un documento de todas sus colecciones tampoco lo toca.

## Decisión

### 1 · Con eventos de puntero, no con el arrastre del navegador

El arrastre se programa con **eventos de puntero** (`pointerdown`,
`pointermove`, `pointerup`), igual que el arrastre del mapa (M5d), y **no se
toca `dragDropEnabled`**. Así:

- **Se comprueba aquí igual que en Windows.** WebView2 es Chromium y los
  eventos de puntero se comportan igual; el arrastre del mapa ya se probó así
  en la laptop del usuario (2026-09-23). Lo que quedaba «sólo para Windows»
  deja de estarlo.
- **La puerta de fuera sigue abierta.** Tauri sigue recibiendo lo que sueltas
  desde el Explorador de Windows, con su ruta. Hoy no se usa, pero es la
  forma natural de *«suelta una carpeta para añadirla»* si algún día se
  quiere.
- El arrastre del navegador se anula en lo que se arrastra (`dragstart`), para
  que una miniatura no empiece otro arrastre por su cuenta.

Un arrastre empieza a los **5 píxeles**: menos es un clic, y el clic, el
doble clic y Ctrl+clic siguen haciendo lo de siempre. El clic que el
navegador manda al soltar se descarta, para que soltar no abra la vista
previa. Sólo con el botón principal del ratón (o un lápiz); con el dedo, la
lista se desplaza como siempre.

### 2 · Qué se lleva

- El documento que agarras. **Si es uno de los elegidos** (Ctrl+clic,
  Mayús+clic, Ctrl+A), **todos los elegidos**. Si no, sólo ése, y lo elegido
  no cambia.
- Mientras arrastras, junto al ratón va una etiqueta con **qué llevas**
  («Pasaporte» o «3 documentos») y **qué va a pasar si sueltas ahí**:
  *«Añadir a «Trámite de la visa»»*, *«Mover a «Todo lo del piso»»*, *«Ya
  está en «Trámite de la visa»»*, *«Crear una colección con él»* o *«Suelta
  en una colección»*. Es la regla de M5d: saber qué va a pasar antes de
  soltar.
- `Esc` cancela el arrastre.

### 3 · Dónde se suelta

- **Cada colección de «Tus colecciones»** en la barra lateral se ilumina al
  pasar por encima.
- **«Nueva colección»** aparece al final de ese grupo sólo mientras arrastras
  (también si todavía no tienes ninguna, y entonces el grupo sale sólo para
  eso). Al soltar, ahí mismo sale un campo para el nombre: `Enter` la crea con
  lo que llevabas, `Esc` o la ✕ lo dejan como estaba.
- Soltar en cualquier otro sitio no hace nada.

No se suelta en la vista «Colecciones» ni en el Inicio: el usuario eligió la
barra lateral, que se ve desde cualquier vista.

### 4 · Añadir o mover

| Desde | Sueltas en | Pasa |
|---|---|---|
| Una carpeta | una colección | **Se añade.** Nada sale de ningún sitio |
| La colección «A» | la colección «B» | **Se mueve**: entra en «B» y sale de «A» |
| La colección «A», con **Ctrl** | la colección «B» | **Se añade** a «B» y sigue en «A» |
| La colección «A» | la misma «A» | Nada: *«Ya está en «A»»* |
| La colección «A» | «Nueva colección» | Se crea y **se mueve** a ella (con Ctrl, se añade) |

Ctrl se puede pulsar o soltar a mitad del arrastre: la etiqueta cambia en el
momento, como el `+` del Explorador de Windows.

**Mover es una sola operación del núcleo**, `Brain::move_between_collections`
(`colecciones::mover`): entra en «B» y sale de «A» **dentro de la misma
transacción**. Si algo no vale (una de las dos ya no existe, o un documento se
fue a la papelera a mitad del arrastre), error y **no cambia nada**: nunca
queda a medias, ni en ninguna de las dos ni en las dos. En el log son dos
respuestas del usuario con la misma hora: un `member_of` nuevo hacia «B» y la
retractación del de «A» (I4: *«¿qué había en «A» el martes?»* sigue
contestándose). A la misma colección no escribe nada.

El aviso lo dice con palabras: *«Movido de «A» a «B». En tu carpeta no cambia
nada.»* La segunda frase contesta la pregunta del usuario cada vez que mueve.

## Alternativas consideradas

### A. El arrastre del navegador (HTML5), apagando `dragDropEnabled`

Menos código: el navegador pinta la imagen que arrastras y cambia el cursor
solo. Rechazada: apaga que Tauri reciba lo que sueltas desde Windows (y
obliga a impedir a mano que un archivo soltado se abra dentro de la ventana),
y sólo se podía comprobar en la laptop. Con punteros, el coste es escribir la
etiqueta que sigue al ratón; lo demás sale igual.

### B. Mover siempre, o añadir siempre

Se le ofrecieron las dos. Eligió mover con Ctrl para copiar: es lo que ya hace
el Explorador de Windows, que es su referencia.

### C. Soltar también en la vista «Colecciones» y en el Inicio

Se le ofreció la vista «Colecciones». No la eligió: con ella abierta no se ve
la carpeta de la que arrastras.

### D. Arrastrar también desde el Inicio, Sensibles o el buscador

Se le ofreció el Inicio y Sensibles. No lo eligió: ahí no se eligen varios. El
buscador tapa la barra lateral, así que no tendría dónde soltar.

## Consecuencias

**A favor**

- Juntar un trámite es agarrar y soltar, como en Windows, sin abrir la vista
  previa de cada documento.
- Reordenar colecciones no deja nada a medias: mover es una transacción.
- Nada nuevo en la configuración de Tauri ni en los permisos de la ventana.

**En contra**

- **Mover saca de la colección de origen.** Es lo que eligió el usuario; si no
  querías, vuelves a arrastrarlo (o con Ctrl se queda en las dos). El log
  recuerda de dónde salió.
- **La etiqueta que sigue al ratón es nuestra**, no la del sistema: no es la
  miniatura translúcida que pinta Windows.
- **No se arrastra fuera de la ventana** (a un correo, a una carpeta de
  Windows). Eso es otra cosa —sacar el archivo real— y no entra aquí.
- Con muchas colecciones, la barra lateral no se desplaza sola al acercarte al
  borde mientras arrastras; la rueda del ratón sí funciona.
