# ADR-0026 — Una fecha de caducidad a mano, donde el detector no vio ninguna

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Completa:** [ADR-0023](0023-vencimientos.md) («En contra»: *todavía no se puede añadir una fecha a mano a un documento donde el detector no vio ninguna*)

## Contexto

El detector de vencimientos (ADR-0023) sólo ve fechas que tienen una palabra
de caducidad delante. Lo que no la tiene —una foto que el OCR no pudo leer,
un documento que dice «válido un año desde la firma», unos apuntes con fecha
de examen— se queda sin fecha, y en la vista previa «cambiar» sólo aparecía
junto a una fecha que ya existía.

En el núcleo no faltaba nada: `Brain::set_expiry` ya escribía una aserción
del usuario sobre cualquier documento, y la tarjeta «Caduca pronto» ya leía la
fecha que manda, fuera de quien fuera. Faltaba **dónde pedirlo**, y eso toca
el diseño. Se le enseñaron al usuario tres sitios (2026-09-23): una línea
tenue en «Lo que sé de este documento», dentro de «Detalles técnicos»
(plegado) o un cuarto botón junto a «Abrir». Eligió **la línea tenue**.

## Decisión

1. En la vista previa, **en los documentos sin fecha**, «Lo que sé de este
   documento» lleva una línea más, en gris: **«Caduca el — añadir»**, en el
   mismo sitio donde saldría la fecha si la hubiera.
2. «añadir» abre el mismo selector que «cambiar», con **guardar** y
   **cancelar** (sin «no caduca»: no hay nada que negar).
3. Lo guardado es una aserción **del usuario** (I5), como una corrección: sale
   con «lo dijiste tú», entra en «Caduca pronto» si vence en 90 días o ya
   venció, y ningún reindexado la toca, porque el detector sólo retracta lo
   suyo.
4. **No sale** donde no tiene sentido: con lo sensible bloqueado por el PIN
   (no se puede corregir lo que no se puede ver, y la línea delataría que no
   hay fecha) ni en lo que está en la papelera (ADR-0025).

## Alternativas consideradas

- **Dentro de «Detalles técnicos».** Cero cambios a la vista; casi nadie lo
  encontraría. Rechazada por el usuario.
- **Un cuarto botón de acción.** La fila de acciones se aprieta y salta de
  línea en el ancho de la vista previa. Rechazada por el usuario.

## Consecuencias

- Una línea más en la vista previa de cada documento sin fecha, en gris para
  que no compita con lo que sí se sabe.
- Nada nuevo en el núcleo ni en el log: la misma aserción que «cambiar».
