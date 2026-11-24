# ADR-0040 — Un contrato se juzga por lo que trae escrito

- **Estado:** Aceptada el 2026-11-24 (JP pidió el ajuste desde el proyecto Ecosistema y dejó fusionar
  con la CI en verde).
- **Fecha:** 2026-11-24
- **Cambia:** [ADR-0006](0006-donde-vive-la-sensibilidad.md), sólo en las señales de nombre y de
  carpeta del clasificador. La columna, la procedencia, la corrección del usuario y la puerta siguen
  igual.
- **Toca:** `obrain-guard` (`rules.rs`, `lib.rs`, `CLASSIFIER_VERSION` pasa a `2`) y el «¿por qué?»
  de la vista previa.

## Contexto

La regla `nombre:contractual` marcaba como sensible cualquier archivo cuyo nombre dijera «contrato»
o «póliza», y la señal de carpeta hacía lo mismo con lo que hubiera dentro de una carpeta
`Contratos`. La puerta de Aetheris en `SoloNormal` ([ADR-0039](0039-la-puerta-de-aetheris-se-adelanta.md))
no consulta lo sensible, ni para el modelo local. La prueba T3 de Aetheris, sobre una biblioteca
inventada, lo dejó a la vista: la póliza de hogar, que sólo habla de coberturas, quedó fuera, y con
ella las tres preguntas que la necesitaban. Con una biblioteca real, Aetheris no podría leer ningún
contrato ni ninguna póliza.

Un pasaporte o una nómina son datos personales por lo que son. Un contrato o una póliza lo son por lo
que traen escrito: el DNI o el NIE de las partes, el IBAN de la domiciliación, una tarjeta. Eso ya lo
detecta `content.rs`, con dígito de control. Las condiciones generales de un seguro, el folleto de
coberturas o un contrato sin datos de nadie no tienen nada que esconder, y son justo los que
responden «¿me cubre la póliza las inundaciones?».

## Decisión

**«Contrato» o «póliza», en el nombre o en la carpeta inmediata, sólo protegen lo que el contenido no
puede contradecir.** Siguen siendo una señal `sensitive`, con su propia regla
(`nombre:contractual`, `carpeta:contexto-contractual`), pero esa señal cede cuando se cumplen las
tres cosas:

1. el archivo tiene texto en `chunks`;
2. ese texto es el de la versión observada: su extracción terminó en `ok` y el hash del archivo que
   leyó es el que observó el escaneo, la misma comparación con la que la extracción decide si un
   archivo cambió. El texto de una versión anterior o de una extracción fallida no cuenta como
   leído;
3. el escaneo de contenido no encontró nada.

Si el contenido encuentra un DNI, un NIE, un IBAN o una tarjeta, el archivo es sensible por eso. Si
no hay texto (un escaneo sin OCR, un archivo deshidratado, una extracción vacía o fallida), la señal
de nombre protege como antes. Cuando cede, la aserción que escribió el clasificador se retracta y
queda en el log con su fecha de baja.

Contrato y póliza son lo último que mira `classify_name`, para que ceder no esconda otra señal del
mismo nombre: `contrato DNI.pdf` es una sigla de identidad y `contrato hipoteca.pdf` es financiero, y
ninguno de los dos cede. Una carpeta `Contratos y DNI` tampoco.

**Las demás señales de nombre no ceden.** Lo que protegen no tiene forma que el contenido pueda
reconocer: el sueldo de una nómina, un diagnóstico, una contraseña escrita a mano. En particular,
«contraseña» en el nombre sigue mandando el archivo a bóveda. Una guía que sólo habla de cómo cambiar
una contraseña se corrige a mano para ese archivo (I5).

## Alternativas consideradas

- **Quitar «contrato» y «póliza» de las reglas.** Rechazada: un contrato escaneado sin texto, que
  casi siempre lleva el NIE en la primera página, quedaría normal y su nombre saldría al proveedor
  remoto por `localizar`.
- **Dejar la regla y que JP corrija cada archivo a mano.** Sigue disponible, pero un control que hay
  que desmentir documento por documento acaba desactivado entero (ADR-0006, «por qué el usuario
  puede bajar el nivel»).
- **Que la puerta dé lo sensible al modelo local.** Es otra decisión, del ADR-0038: la puerta no puede
  comprobar quién lee lo que entrega. No se toca aquí.

## Consecuencias

- La póliza y el contrato inventados de T3 pasan a normal y la puerta los da: comprobado el
  2026-11-24 armando la biblioteca de T3 con el ayudante de la batería
  (`tests/bateria_aetheris.rs`), donde ahora coinciden con la puerta simulada. En la batería, la
  póliza hostil del caso A1 también llega a la puerta, así que ese ataque se prueba de verdad. En una biblioteca real,
  los contratos y las pólizas que traen el NIE o un IBAN siguen fuera: se abren las condiciones
  generales, las coberturas y lo que no lleva datos de nadie.
- El nombre de un contrato que pasa a normal sale al proveedor remoto por `localizar`, como el de
  cualquier documento normal (decisión 0006 de Aetheris). Su texto sólo llega al modelo local.
- **Lo que el contenido no ve no lo protege.** Un NIE que el OCR leyó mal no pasa el dígito de
  control, y el OCR sólo lee las primeras 20 páginas de un PDF (`MAX_PDF_PAGES`). Un contrato así
  puede quedar normal con datos dentro. Ese texto sigue sin salir de la máquina; lo que sale es su
  nombre. Para lo que importe, la marca de carpeta (ADR-0020) o la corrección a mano siguen ahí.
- `CLASSIFIER_VERSION` sube a `2`. Lo que el clasificador ya había dicho y sigue sosteniendo no se
  reescribe: sólo cambia la fuente de lo que escribe a partir de ahora.
