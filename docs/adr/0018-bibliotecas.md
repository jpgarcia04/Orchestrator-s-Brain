# ADR-0018 — Bibliotecas: el árbol empieza en la carpeta que eliges, y cada carpeta se llama por su nombre

- **Estado:** Aceptada
- **Fecha:** 2026-09-22
- **Acota:** [ADR-0013](0013-el-mapa-es-la-pantalla.md), [ADR-0014](0014-dos-vistas-un-solo-estado.md), [ADR-0015](0015-el-cielo-reparte-por-anchura-no-por-peso.md)

## Contexto

El proyecto se diseñó para entender **todo el disco** del usuario. El usuario lo
replanteó el 2026-09-22: lo que quiere es *«un segundo cerebro, no un segundo
disco duro»*, alimentado con documentos personales que él elige uno a uno y
deja en una carpeta concreta.

Al abrir la aplicación contra un corpus así aparecieron dos defectos que no eran
de pintura sino del árbol que produce el núcleo:

1. **El árbol empezaba en la raíz del disco.** M3 encadena cada carpeta con su
   madre hasta el volumen, así que para llegar a la bóveda había que bajar por
   `C:\Users › Users\pacox › pacox\OneDrive › OneDrive\Documentos…`. En el
   banco de pruebas, trece niveles hasta la primera carpeta con documentos.
2. **Todas las carpetas se llamaban con dos segmentos.** `obrain-link` nombraba
   `padre/hoja` para que cinco carpetas `src` de un repositorio no fueran cinco
   nodos idénticos. El remedio lo pagaban todas: el cielo decía
   `Users/pacox (resto)` y `OneDrive/Documentos`, y las migas repetían el padre
   en cada escalón. El usuario: *«de nada me sirve que todos los nodos se llamen
   pacox/documentos… no es intuitivo y se ve mal»*.

## Decisión

**Una carpeta que el usuario elige indexar es una biblioteca, y el árbol empieza
en ella.**

1. **La biblioteca es la entidad de carpeta de siempre con la faceta
   `library`**, afirmada por el usuario (I5): elegir qué carpeta es tu bóveda es
   una decisión tuya, no una observación del disco. La registra el escáner al
   recorrer una raíz (`obrain_scan::register_library`).
2. **M3 no encadena por encima de una biblioteca.** Lo que hay encima es el
   camino del disco hasta ella, no conocimiento del usuario.
3. **La lectura del árbol corta en las bibliotecas aunque el log diga otra
   cosa.** Una base escaneada antes de esta decisión tiene aristas hasta
   `C:\Users`; `sky::read_tree` las ignora para las bibliotecas y descarta las
   raíces que se quedan vacías. Así basta con marcar la carpeta: no se reescribe
   el log (I4).
4. **Con una sola raíz, el explorador entra en ella.** Una lista de un único
   elemento es un clic de más.
5. **Una carpeta se llama por su nombre.** El choque entre dos que se llaman
   igual se resuelve **donde ocurre** —en la vista que las enseña juntas— con su
   madre entre paréntesis: `Documentos (casa)`, `Documentos (trabajo)`. Sólo esas
   pagan el aclarado.
6. **Indexar es una sola operación** (`obrain_api::indexar`): escanear,
   enlazar, extraer, clasificar y vectorizar, en ese orden, con las mismas
   transacciones que los cinco CLI. La usan `obrain indexar <carpeta>` y la
   aplicación, que reindexa sus bibliotecas al arrancar y con «actualizar», en
   segundo plano y con su propia conexión (la base pasa a modo WAL).

## Consecuencias

- El explorador abre en tu bóveda; las migas dicen `inicio › Documentos ›
  Personal › Salud` y no la ruta del disco.
- **M5c pierde la mitad de su motivo.** El USN Change Journal existía para
  vigilar un volumen entero; con bibliotecas basta con vigilar esas carpetas, y
  mientras tanto el reindexado al arrancar cubre el hueco (es idempotente: sin
  cambios no escribe nada).
- Los CLI de cada etapa siguen existiendo y siguen siendo la forma de auditar
  (`--sample`). `obrain-scan` también registra su raíz como biblioteca.
- **Una corrección durante un reindexado espera.** Leer no se bloquea (WAL),
  pero escribir sí: si el indexador está vectorizando dentro de su transacción,
  un «es correcto» del panel espera a que termine (hasta 10 s). Partir la
  vectorización en transacciones más pequeñas lo resuelve cuando haga falta.
- **Lo que no cubre:** quitar una biblioteca. Hoy una carpeta que desaparece del
  disco se informa (`missing`) y se deja como estaba en el log. Retirarla de
  verdad es una retractación de la faceta y necesita su gesto en la interfaz.
