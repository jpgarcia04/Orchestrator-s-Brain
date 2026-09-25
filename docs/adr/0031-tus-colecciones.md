# ADR-0031 — Tus colecciones

- **Estado:** Aceptada
- **Fecha:** 2026-09-24
- **Resuelve:** la pregunta del maestro *«¿Puede el usuario crear sus propias regiones y nombrarlas?»* (§10), cuyo planteamiento decidió el usuario el 2026-09-23
- **Toca:** [ADR-0025](0025-lo-que-ya-no-esta.md) (la papelera), [ADR-0030](0030-un-documento-dentro-de-otro.md) (los paquetes), [ADR-0020](0020-sensibilidad-por-carpeta.md) (de dónde sale la sensibilidad), [ADR-0014](0014-dos-vistas-un-solo-estado.md) (el mapa y el explorador)

## Contexto

El 2026-08-10 el usuario lo pidió así: *«quiero poder ordenar mis cosas a mi
gusto y que el programa no pierda de vista qué es qué y con qué se
relaciona»*. Hoy todo lo que agrupa en obrain sale del disco: la carpeta de
cada documento (`contained_by`), y en el mapa cada estrella es una carpeta.
Pero lo que una persona junta para un trámite no vive en una carpeta: el
«Trámite de la visa» lleva el pasaporte (`01 Identidad`), los estados de
cuenta (`06 Dinero`) y la carta del consulado (`04 Trámites`).

El 2026-09-23 el usuario decidió el planteamiento (maestro §10): una región del
usuario es **una entidad con faceta `region`** que crea él (I5); un documento
puede estar en **varias** —es una colección, no una carpeta, así que no hay
que decidir en cuál cae—; el mapa se sigue dibujando por carpetas y la
colección se ve como **una lente encima, sin mover estrellas**. En la
interfaz se llama **colección**.

Quedaba lo concreto. Se le enseñó al usuario el 2026-09-24, con su coste y en
dos tandas cortas, antes de escribir código:

| Pregunta | Eligió |
|---|---|
| ¿Cómo se meten documentos? | **Desde la vista previa, y varios a la vez** en el explorador (Ctrl+clic, Mayús+clic). No arrastrar: en Windows, Tauri se queda con el arrastre para soltar archivos de fuera, y sólo se podría comprobar en su laptop |
| ¿Dónde se ven? | **En la barra lateral** y en **una vista propia «Colecciones»**. No en el Inicio |
| Con una colección abierta, ¿el buscador busca sólo dentro? | **Sí, con un filtro** que se quita con un clic |
| ¿Una colección puede ser sensible? | **No: sólo agrupa.** No destapa ni protege nada |
| Si metes un paquete, ¿entra lo que lleva dentro? | **No: sólo lo que metes tú** |
| ¿Se puede meter una carpeta «en vivo»? | **No: sólo documentos.** Con la selección múltiple se mete de golpe lo que hay hoy en una carpeta |

Y se le dijo lo que se haría si no decía otra cosa, porque sale de lo que ya
había decidido: la papelera, borrar una colección, el mapa y renombrar (abajo).

## Decisión

### 1 · Qué es una colección

Una entidad nueva, sin archivo detrás, con tres clases de aserciones, **todas
del usuario** (`method = user`, I5: ningún reescaneo las toca porque ningún
enlazador las escribe):

- **`has_facet = region`**: es una colección. Es lo que la hace aparecer.
- **`collection_name`** («se llama»), cardinalidad `one`: su nombre.
  Renombrar es otra respuesta del usuario, que retracta la anterior
  (`Store::assert_by_user`). Sin repetidos entre las vivas (sin distinguir
  mayúsculas), y sin nombre vacío.
- **`member_of`** («está en la colección»), del documento a la colección,
  cardinalidad `many`. Sujeto el de dentro, como `contained_by` y `part_of`.
  Su inverso, `has_member` («reúne»), se declara pero **no se almacena**
  (§3.4). Sacar un documento **retracta** su `member_of`: el log recuerda que
  estuvo y hasta cuándo (I4).

Sólo entran **documentos**: algo con una ruta vigente. Ni carpetas, ni
colecciones dentro de colecciones, ni lo que está en la papelera.

No se reutiliza `contained_by` (es el árbol del disco: el mapa dibujaría la
colección como una carpeta) ni `part_of` (es el texto de un documento dentro
de otro, ADR-0030).

La faceta se llama `region` porque así se decidió en §10 y porque es la
palabra del modelo (*regiones del usuario*); en la interfaz, en la terminal y
en los documentos para el usuario es **colección**. No hay que confundirla con
las regiones del cielo (`sky::Region`), que son carpetas.

### 2 · Mover, renombrar, borrar

- **Mover o renombrar un documento no lo saca de su colección.** La colección
  apunta a la entidad, no a la ruta (I8): el mismo documento en otra carpeta
  sigue siendo el mismo.
- **La papelera (ADR-0025).** Lo que está en la papelera sale de la colección
  como sale de todo lo demás, y la colección dice **«1 en la papelera»**. Su
  `member_of` no se toca: si vuelve (lo sacas de la Papelera de Windows, o
  aparece el mismo contenido), **vuelve solo a sus colecciones**, igual que
  vuelve con sus correcciones. Lo olvidado a los 30 días deja de contarse, y el
  log sigue sabiendo que estuvo.
- **Borrar una colección** pide dos clics, como quitar la marca de una
  carpeta. Se retractan su faceta, su nombre y sus `member_of`. **Tus
  documentos no se tocan**: ni tus archivos, ni sus carpetas, ni las otras
  colecciones donde estén. El log recuerda que existió, qué tenía y hasta
  cuándo (I4: *«¿qué junté para la visa?»* es una consulta bitemporal).

### 3 · Paquetes (ADR-0030): sólo lo que metes tú

Meter un paquete —el PDF de la visa con tus estados de cuenta dentro— mete el
paquete y nada más. En la colección se ve como en todas partes: si también
metiste uno de los que lleva dentro, **sale justo debajo de su paquete**, un
poco metido, igual que en la búsqueda. La colección no cambia sola cuando
obrain descubre otra parte.

### 4 · Sensibilidad: una colección sólo agrupa

Una colección **no cambia la sensibilidad de nada**. No es una cuarta fuente
en `Store::resolve_sensitivity`: lo sensible sigue tapado dentro de ella
igual que en su carpeta, con el PIN igual, y la colección no lo destapa ni lo
protege más. La entidad de la colección es `normal`: su nombre lo escribiste
tú y no sale de ningún documento.

Se descartó marcarla como una carpeta (alternativa B): sacar un documento de
la colección le quitaría la protección sin avisar.

### 5 · Lo que se ve

- **Barra lateral.** «Colecciones» entre «Mapa» y «Sensibles», con cuántas
  hay, lleva a la vista propia. Debajo de «Tu bóveda», el grupo **«Tus
  colecciones»** con cada una y cuántos documentos tiene; un clic la abre.
- **Vista «Colecciones».** Todas, con cuántos documentos tiene cada una,
  cuántos en la papelera y desde cuándo existe; **«Nueva colección»**.
- **Una colección abierta** se ve como una carpeta: cuadrícula o lista, un clic
  la vista previa, dos clics lo abre. Cada documento dice **su carpeta de
  verdad**, porque vienen de sitios distintos. En la cabecera: «Ver en el
  mapa», «Renombrar» y «Borrar» (dos clics).
- **Vista previa.** Junto a la carpeta del documento, un chip por cada
  colección donde está (un clic la abre), y el botón **«Colecciones»**: una
  lista para marcar en cuáles está y crear una nueva ahí mismo.
- **Varios a la vez.** En el explorador y en una colección, Ctrl+clic suma o
  quita uno, Mayús+clic suma un tramo y Ctrl+A los elige todos. Abajo sale
  una barra: *«3 elegidos · Añadir a una colección»* (en una colección,
  también *«Sacar de «Trámite de la visa»»*). Un clic normal sigue siendo la
  vista previa, y `Esc` suelta la selección.
- **Buscar dentro.** Con una colección abierta, el buscador sale con el
  filtro **«Sólo en «Trámite de la visa»»**; un clic en la ✕ busca en todo.
  El filtro se aplica **antes** de cortar la lista, en las dos ramas (BM25 y
  el recorrido de vectores), así que no se pierden resultados que estaban más
  abajo. Elegir un resultado lo enseña sin salir de la colección.
- **Mapa: la lente de una colección.** «Ver en el mapa» dibuja el cielo de
  siempre y **enciende las estrellas donde están sus documentos**; las demás
  se atenúan. No se mueve ninguna estrella: es lo mismo que ya hacía el mapa
  para encender los resultados de una búsqueda (`Mapa.encender`). Cada
  documento enciende la estrella más cercana que lo contiene.
- **Mapa desde un documento.** «Ver en el mapa» de un documento dibuja lo que
  se relaciona con él; ahora eso incluye sus colecciones y, a dos saltos, lo
  que juntaste con él. Pulsar la colección la abre.
- **Terminal.** `obrain colecciones` las lista; `obrain coleccion "<nombre>"`
  enseña lo que tiene, y con `--crear`, `--meter <archivo>…`,
  `--sacar <archivo>…`, `--renombrar "<nuevo>"` o `--borrar` la cambia.

## Alternativas consideradas

### A. Una colección como carpeta virtual del árbol (`contained_by`)

Reutilizaría el explorador y el mapa sin tocar nada. Rechazada: un documento
tendría dos carpetas, el árbol dejaría de ser un árbol y el mapa movería
estrellas, que es justo lo que el usuario dijo que no.

### B. Una colección sensible, como una carpeta marcada

Se le ofreció. Rechazada por el usuario: sería otra fuente de sensibilidad, y
sacar un documento de la colección le quitaría la protección sin avisar.

### C. Carpetas en vivo dentro de una colección

«Todo lo del piso» = `05 Vivienda` entera, y lo que llegue después. Se le
ofreció. Rechazada por el usuario: dos maneras de estar en una colección y
excepciones para sacar uno. La selección múltiple cubre el caso de hoy.

### D. Que un paquete meta lo que lleva dentro

Se le ofreció. Rechazada por el usuario: la colección cambiaría sola.

### E. Arrastrar documentos a la colección

Se le ofreció. Rechazada por ahora: en Windows, Tauri se queda con el
arrastre (`dragDropEnabled`) para cuando sueltas archivos de fuera; usarlo
dentro de la ventana obliga a cambiar eso, y sólo se puede comprobar en su
laptop. Se puede añadir después sin cambiar nada de esto.
**Añadido el 2026-09-25 en [ADR-0033](0033-arrastrar-a-una-coleccion.md)**,
con eventos de puntero y sin tocar `dragDropEnabled`.

### F. Filtrar la búsqueda después de cortarla

Buscar como siempre (los 12 primeros) y quitar lo que no está en la
colección. Más sencillo, y rechazado: si los de la colección estaban en el
puesto 30, la lista saldría vacía aunque hubiera respuesta.

## Consecuencias

**A favor**

- Lo que juntas para un trámite se queda junto, venga de la carpeta que venga,
  y sigue junto aunque muevas o renombres los archivos.
- Es todo tuyo y está en el log: *qué junté, cuándo, y qué saqué* se puede
  preguntar siempre (I4, I6).
- No toca la puerta de sensibilidad, ni el árbol, ni las estrellas del mapa.
- La búsqueda dentro de una colección no pierde resultados.

**En contra**

- **No entra nada solo.** Lo que llegue después a una carpeta, o la parte que
  obrain descubra dentro de un paquete, no entra en la colección hasta que lo
  metas tú. Es lo que el usuario eligió.
- **Una colección no protege.** Si juntas tu pasaporte con otras cosas, lo
  protegido sigue siendo el pasaporte (por su carpeta o por su texto), no la
  colección.
- **Borrar una colección no se deshace con un clic.** El log la recuerda,
  pero volver a tenerla es crearla otra vez. Por eso pide dos clics.
- **Un documento borrado a mano del todo** (olvidado) sigue en el log como
  miembro; no se ve, y si su contenido vuelve algún día, vuelve a su colección.
