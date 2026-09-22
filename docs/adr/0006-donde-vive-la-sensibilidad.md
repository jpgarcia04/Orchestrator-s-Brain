# ADR-0006 — La sensibilidad es una columna impuesta con procedencia en el log

- **Estado:** Aceptada — su alternativa D la acota [ADR-0020](0020-sensibilidad-por-carpeta.md) (2026-09-22): el **usuario** puede marcar una carpeta y lo de dentro hereda ese nivel como suelo. Como señal del clasificador, D sigue igual.
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0001](0001-log-de-aserciones-como-fuente-de-verdad.md), [ADR-0005](0005-donde-vive-el-texto-extraido.md)

## Contexto

El invariante I11 dice que cada entidad tiene un nivel de sensibilidad y que cada capacidad de IA declara si puede salir de la máquina. M0 dejó la columna `entities.sensitivity` con un comentario que zanjaba la cuestión en una frase:

> *Única excepción a "todo es una aserción": un control de seguridad no puede depender de resolver una proyección.*

Al implementar el clasificador, esa frase resultó ser sólo la mitad de la respuesta. Una columna suelta no puede contestar tres preguntas que el resto del sistema da por hechas:

1. **¿Por qué crees que esto es una clave?** — I9 exige procedencia para toda afirmación. `sensitivity = 'vault'` no dice quién lo decidió ni con qué regla.
2. **¿Y si te equivocas?** — I5 exige que la corrección del usuario sea permanente. Un `UPDATE` lo deja igual de permanente que la siguiente pasada del clasificador, es decir: nada.
3. **¿Qué sabías en marzo?** — un control de seguridad sin historial no es auditable, y la auditoría era gratis en este modelo de datos.

La tentación evidente es mover la sensibilidad al log y calcularla como cualquier otra proyección. La tentación opuesta es dejarla como columna y renunciar a las tres. Ninguna de las dos es correcta.

## Decisión

**El nivel se escribe en los dos sitios, con papeles distintos y no negociables:**

| Dónde | Papel | Quién lo lee |
|---|---|---|
| `entities.sensitivity` (columna) | **El punto de control.** Es lo único que decide si algo se vectoriza o sale. | `gate::check`, `gate::embeddable` |
| Aserciones `sensitivity_level` (log) | **La procedencia y el canal de corrección.** Qué regla disparó, con qué confianza, cuándo, y qué dijo el usuario. | la auditoría, la UI, el propio clasificador |

El puente entre ambos es una única función, `Store::reproject_sensitivity`, con dos reglas en este orden:

1. **Si hay una aserción del usuario vigente, gana** — incluida la que *baja* el nivel.
2. **Si no, gana la señal más restrictiva** de las que el clasificador sostiene.

Ninguna comprobación de egreso resuelve nunca el log. Lee una columna con un `CHECK` que sólo admite tres valores. Esa asimetría es la decisión entera.

### Por qué el usuario puede bajar el nivel

Es la parte incómoda y es deliberada. Un clasificador demasiado celoso al que no se puede contradecir no produce un sistema más seguro: produce un usuario que desactiva la bóveda entera porque le ha sacado del buscador media carpeta de trabajo. La corrección queda en el log como aserción de método `user`, con su marca temporal, y ninguna pasada posterior puede retractarla —el clasificador sólo retracta lo que él mismo escribió—. Bajar el nivel es una decisión de seguridad **registrada**, que es distinto de una decisión de seguridad invisible.

### Por qué la evidencia nunca contiene el texto encontrado

Una aserción es inmutable (I1) y no se borra (I4). Un clasificador que copiara en `evidence` la clave de API que acaba de detectar habría creado una fuga permanente mientras intentaba evitar una. Lo que se guarda es **una dirección, no una cita**: `regla=contenido:variable-con-secreto fragmento=7`. El tipo `Signal` no tiene siquiera un campo donde meter el texto, para que el error no se pueda cometer por descuido.

## Alternativas consideradas

### A. Sólo la columna, sin log

Es lo que había. Rechazada porque hace imposibles la procedencia, la corrección permanente y la auditoría, que son tres cosas que este modelo de datos regala en todos los demás casos. Además convierte el clasificador en un proceso no idempotente: sin saber qué dijo la vez anterior, no puede distinguir «esto ya lo sabía» de «esto ha cambiado», y reclasificar sería reescribir a ciegas.

### B. Sólo aserciones, resolviendo el nivel al consultar

La opción coherente con «todo es una aserción», y por eso la que había que descartar por escrito.

Rechazada por la forma del fallo. Resolver el nivel exige una consulta bitemporal con precedencia de método sobre una tabla que crece sin límite. Si esa consulta falla, devuelve vacío o llega tarde, el valor por defecto de un `Option` es lo que decide si un pasaporte se envía a un proveedor remoto. Un control de seguridad tiene que fallar cerrado por construcción, no porque alguien se acordara de poner el `unwrap_or` correcto en los once sitios donde se llama. Con la columna, el peor caso es un `SELECT` de una fila que o devuelve uno de tres valores o es un error.

Hay además un argumento de coste: la puerta se consulta una vez por entidad y por operación. En una pasada de embeddings sobre 200.000 documentos eso son 200.000 resoluciones del log frente a 200.000 lecturas de una columna indexada.

### C. La columna como caché, recalculada perezosamente al leer

Elimina la posibilidad de que columna y log se desincronicen. Rechazada porque devuelve el problema de B por la puerta de atrás: si la lectura puede recalcular, la lectura puede fallar. Y porque la desincronización se ataca mejor donde está el riesgo real —las dos escrituras van en la misma transacción, y la columna se reconstruye entera con una pasada del clasificador, igual que cualquier otra proyección de I1—.

### D. Sensibilidad heredada por carpeta, en vez de por entidad

Más simple de explicar a un usuario: «esta carpeta es privada». Rechazada como **modelo**, aceptada como **señal**: la carpeta contribuye una señal de contexto que llega hasta `sensitive` y nunca hasta `vault` (`rules::classify_parent`). Como modelo fallaba en los dos sentidos —un archivo secreto en `Descargas` se quedaba fuera, y una carpeta `Contratos` teñía los veinte mil archivos que colgaran de ella—, y sobre todo chocaba con I8: la ruta es una observación, no la identidad. Colgar un control de seguridad de la ruta significa que mover un archivo cambia sus permisos.

## Consecuencias

**A favor**

- **La política de §5.2 es código que se prueba.** `gate::decide` es una función pura; la tabla del maestro y sus tests son la misma cosa.
- **Falla cerrado por construcción.** Una entidad desconocida o un valor ilegible se tratan como `vault`. Hay un test que lo comprueba con una entidad inexistente.
- **El clasificador es idempotente y reversible.** Segunda pasada: 0 escrituras. Cambiar una regla retracta lo que ya no se sostiene y escribe lo nuevo; la columna se recalcula sola.
- **La corrección del usuario es real.** No es una preferencia que el siguiente reescaneo pisa: es una aserción con precedencia permanente, y el clasificador tiene prohibido tocarla.
- **La auditoría sale gratis, otra vez.** «¿Qué archivos consideraba secretos el sistema en marzo?» es una consulta bitemporal sobre `sensitivity_level`.

**En contra**

- **Hay dos sitios que pueden divergir.** Igual que en ADR-0005, van en la misma transacción pero no lo impone el motor. Un `UPDATE entities SET sensitivity` a mano en una consola deja la columna diciendo algo que el log no sostiene, y la siguiente reproyección lo revierte sin avisar. Es una debilidad real.
- **La columna no tiene bitemporalidad.** Sólo refleja el presente. Reconstruir el control de egreso tal y como estaba en una fecha pasada exige reproyectar desde el log con ese `TimePoint`, que se puede hacer pero no es lo que lee la puerta.
- **`Sensitivity` ordena por restricción** (`Normal < Sensitive < Vault`) y la regla «gana el máximo» depende de ese orden. Añadir un nivel en medio en el futuro es una decisión que hay que tomar mirando este `derive(Ord)`.
- **La bóveda todavía no está cifrada.** §8.1 pide para `vault` una clave derivada de una frase de paso que no se almacena. Esto decide *quién queda dentro*; no protege el contenido en reposo más que el resto de la base. La etiqueta llega antes que la cerradura, y así hay que contarlo.
