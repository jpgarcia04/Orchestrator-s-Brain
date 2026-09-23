# ADR-0028 — Vencimientos sin palabra: la zona MRZ, el plazo exacto y el año solo

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Completa:** [ADR-0023](0023-vencimientos.md) (§1: *sólo cuenta una fecha con una palabra de caducidad delante*)

## Contexto

La primera prueba de verdad de los vencimientos fue el **pasaporte mexicano**
del usuario (2026-09-23): un PDF de una página, la carátula que le pidió a
ChatGPT que recortara de su pasaporte completo. No salió ninguna fecha de
caducidad.

Se miró por qué **sin copiar sus datos a ningún sitio**: un diagnóstico que
enseñaba la *forma* del texto reconocido (letras → `x`, cifras → `9`, salvo
las etiquetas conocidas). Lo que pasó:

- El OCR de Windows (es-MX) **sí** leyó el documento, y encontró sus tres
  fechas, `99 99 9999`: nacimiento, expedición y caducidad.
- Pero se comió las etiquetas. De «Fecha de caducidad / Date of expiry» quedó
  «Fecha de» y «date»; de la de nacimiento, «Fecha de Date ot bith». Se
  probó a pintar la página a otras resoluciones, de 764 a 7.281 px de ancho:
  en ninguna aparece «caducidad», ni «expiry», ni «birth».
- Tampoco trae **zona MRZ** (las líneas `P<MEX<<…` del pie): o el recorte la
  dejó fuera o el OCR no la vio.

Sin palabra, el detector de ADR-0023 no se fía de ninguna fecha, y hace bien:
un documento está lleno de fechas. Pero había una huella clara: **la tercera
fecha es exactamente la segunda más 10 años, el mismo día y mes**. Así caduca
un pasaporte: la expedición más su validez.

Se le enseñaron al usuario las opciones y eligió (2026-09-23):

| Pregunta | Eligió |
|---|---|
| ¿Cómo encontrar la caducidad cuando falta la palabra? | **Plazo exacto**, en documentos de identidad (no sólo en lo que dice «pasaporte»; tampoco «no adivinar» y ponerla a mano) |
| ¿Algo más, de paso? | **La zona MRZ** y **el año solo de la INE** («VIGENCIA 2023 - 2033») |

## Decisión

Tres lecturas nuevas, que se suman a las palabras de ADR-0023. Todas escriben
la misma aserción (`expires_on`, una por documento, corregible) y compiten en
el mismo `elegir`: gana el peso y, si empatan, la fecha más lejana.

| Regla | Qué lee | Peso | Confianza |
|---|---|---|---|
| `mrz` | la caducidad de la zona de lectura mecánica, **con su dígito de control** | **4** | 0,95 |
| `caducidad`, `validez`, `vencimiento`, `vigencia`, `hasta` | las de ADR-0023 (ahora también con un año solo) | 3 · 2 · 1 | 0,85 … 0,55 |
| `plazo` | dos fechas a N años justos en un documento de identidad | **1** | 0,70 |

### 1 · La zona MRZ manda

- Se leen las segundas líneas del **TD3** (pasaportes, 2 × 44 caracteres; el
  TD2 de algunos visados tiene la misma) y del **TD1** (tarjetas: TIE, DNI,
  INE, 3 × 30). La caducidad es un campo `AAMMDD` en un sitio fijo.
- Sólo cuenta si cuadran **los dos dígitos de control** de ICAO 9303 (pesos
  7-3-1, módulo 10): el del nacimiento y el de la caducidad. Un texto
  cualquiera no pasa por casualidad, y por eso no se exige ningún `<` (el OCR
  a veces los lee como `K`).
- Tolera el ruido del OCR: se saltan los espacios, `«` y `‹` valen `<`, y en
  los campos numéricos una `O` es un 0, una `I` un 1… El dígito de control
  dice si era eso.
- **Pesa más que cualquier palabra**: si el OCR leyó «VALIDEZ 15 11 2076» en
  la cara de una TIE y el dorso dice 2026 con su control, gana el dorso. Es la
  única lectura que detecta su propio error.
- Años de caducidad: `AA` ≥ 70 es 19AA; si no, 20AA.

### 2 · El plazo exacto, sólo en documentos de identidad

- **Qué:** dos fechas **con día y con el año en cuatro cifras** a entre 1 y
  10 años justos —el mismo día y mes, o la víspera, que hay países que caducan
  así; el 29 de febrero cumple el 28—. La más tardía es la caducidad.
- **Dónde:** sólo en un texto que diga *pasaporte*, *passport*, *identidad*,
  *identity card*, *residencia*, *residence*, *visa*, *visado*, *credencial*
  o *licencia de conducir* (y *driving licence*). Se mira por fragmento, que
  en un documento de identidad es el documento entero.
- **Por qué tan acotado:** lo que caduca se queda en «Caduca pronto» hasta que
  se corrige (ADR-0023 §5), así que una falsa alarma cuesta. Se comprobó
  contra la base de agosto del usuario (561 documentos con texto) y la primera
  versión dio **una**: un `package-lock.json`, por `"license": "MIT"` y
  números de versión como `1.10.20` y `1.10.30`, que para el lector de fechas
  son el 1 de octubre de 2020 y de 2030. De ahí «licencia» sólo de conducir y
  el año en cuatro cifras. Con eso, esa base no cambia en nada y la actual
  sólo gana la fecha del pasaporte.
- **Pesa lo mínimo:** si alguna palabra dijo algo, gana la palabra.
- **La evidencia dice cuántos años:** `regla=plazo fragmento=0
  bytes=736..804 anios=10`. Los años no son una cita del documento: son la
  resta de dos fechas. El tramo va de una fecha a la otra, así que el
  «¿por qué?» marca las dos: *«No leí la palabra «caducidad» —el OCR no la
  vio—, pero es un documento de identidad y dos de sus fechas están a 10 años
  justos: la de expedición y ésta, como en un pasaporte. Las dos están
  marcadas arriba.»*

### 3 · El año solo, tras una palabra de caducidad

- «VIGENCIA 2033», «VIGENCIA 2023 - 2033» (o `2023-2033`), «válida hasta
  2031»: **el 31 de diciembre** de ese año, como el último día del mes en
  «CADUCA 12/2028». En un intervalo de años, cuenta el segundo.
- No tras un «hasta» suelto, la palabra más débil y la que más cantidades
  lleva detrás («cobertura hasta 2000…»).
- No si le sigue una moneda o una unidad («2000 euros», «2000 €», «2027
  días») ni si va pegado a otro número («2027/05», «2027.5»). Fuera de
  1990–2100, como siempre, no es un año de documento.

### 4 · El detector pasa a `expiry@v2`

Lo que escribió la v1 sigue siendo suyo: el detector reconoce lo propio por
`expiry@`, así que una fecha que la v2 lee igual (valor y evidencia) no se
reescribe, y una que lee distinta se retracta y se afirma de nuevo con la
procedencia nueva (I4, I9). Lo del usuario, como siempre, no se toca.

## Alternativas consideradas

### A. Leer mejor la etiqueta: más resolución o comparar palabras parecidas

Se probó la resolución y no sirvió: a ningún tamaño aparece «caducidad». Y
buscar palabras *parecidas* no rescata una palabra que el OCR no escribió.

### B. En un documento de identidad, la fecha más lejana

Más sencilla que el plazo. Rechazada: sin la resta, cualquier fecha futura
de un documento que mencione un pasaporte (una cita, un plazo de trámite)
sería su caducidad, y el «¿por qué?» no tendría nada que explicar.

### C. Sólo en lo que diga «pasaporte»

La opción más prudente, que se le ofreció al usuario. Rechazada por él: no
ayuda con la TIE, la INE ni la licencia, que son los siguientes que meterá.

### D. No adivinar: ponerla a mano

Ya existe («Caduca el — añadir», ADR-0026) y sigue ahí para lo que ninguna
regla vea. Rechazada como respuesta a esto: el documento *sí* dice cuándo
caduca, y un sistema que lo tiene delante y no lo ve no es de fiar.

### E. Un modelo que clasifique las fechas

Igual que en ADR-0023: el modelo local es de embeddings, no de extracción, y
una fecha que no sabe decir por qué la eligió incumple I9.

## Consecuencias

**A favor**

- El pasaporte del usuario tiene su fecha, con un «¿por qué?» que se entiende
  y las dos fechas marcadas en el texto.
- Lo que tenga MRZ —pasaportes, el dorso de la TIE, del DNI o de la INE— sale
  con la fecha más fiable que hay, y un número mal leído se descarta en vez de
  convertirse en otra fecha.
- La INE sale con su vigencia aunque sólo diga años.
- Todo sigue siendo auditable y sin texto del documento en el log.

**En contra**

- **El plazo puede equivocarse** en un documento de identidad con dos fechas
  a años justos que no sean expedición y caducidad (un trámite que cita el
  nacimiento de un niño de 5 años y la fecha de su quinto cumpleaños, por
  ejemplo). Pesa lo mínimo, se ve pálida (0,70), su «¿por qué?» enseña las dos
  fechas y se corrige en un clic.
- **Una MRZ sin segunda línea legible no sirve**: el dígito de control es
  todo o nada. Si el OCR la lee a medias, la fecha sale por las otras vías o
  no sale.
- **El año solo pierde el día**: una tarjeta que diga «VIGENCIA 2033» y caduque
  en junio saldrá en diciembre. Es lo que dice el documento; si se sabe más,
  «cambiar».
- Las reglas se miran **por fragmento**: un documento de identidad largo en el
  que la palabra «pasaporte» y las dos fechas caigan en fragmentos distintos
  no se detecta. Los documentos de identidad son cortos; si aparece uno, se
  sube a documento entero.
