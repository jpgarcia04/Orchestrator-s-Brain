# ADR-0007 — La identidad se reconcilia por ruta cuando el `FileID` se rompe

- **Estado:** Aceptada
- **Fecha:** 2026-08-07
- **Depende de:** [ADR-0001](0001-log-de-aserciones-como-fuente-de-verdad.md)
- **Contesta a:** el invariante I8

## Contexto

El invariante I8 dice:

> *La identidad de un archivo no es su ruta, es `(volumen, FileID) + hash`. Sobrevive a mover, renombrar y desconectar discos.*

Sobrevive a las tres. **No sobrevive al guardado atómico.** Un editor que escribe en un temporal y renombra encima —que es como guardan VS Code, Word, Excel, `git checkout` y la práctica totalidad de las aplicaciones modernas— deja en la ruta un archivo con un `FileID` de NTFS distinto. El escaneo siguiente no reconoce nada y crea una entidad nueva. La vieja se queda con la misma ruta viva, el contenido caducado y sus fragmentos.

Lo destapó el informe de la puerta de sensibilidad: al corregir `search.rs` y reescanear, el veredicto no cambiaba, porque el veredicto era el de la entidad fantasma.

```
7838c90638c8c3fa:b6700000000055010000000000000000   ← antes de editar
7838c90638c8c3fa:e3320200000022000000000000000000   ← después
```

Para la puerta el fallo era **hacia el lado seguro**: el fantasma conserva su nivel antiguo y el archivo nuevo se clasifica de cero. Para el índice vectorial no lo es. Son dos juegos de vectores del mismo documento, y la búsqueda semántica devuelve el archivo dos veces, una de ellas con texto que ya no existe en el disco. Por eso esto tenía que resolverse antes de M4 y no después.

**La medición que obligó a decidir.** Una base recién creada tiene cero fantasmas por construcción, así que ese número no dice nada. Lo que mide es reescanear: se editó **un** archivo y se reescaneó con el código anterior sobre la misma base. Resultado: **una** entidad fantasma. La tasa es de uno por archivo editado por reescaneo, y no tiene techo — en un repositorio en desarrollo, semanas.

## Decisión

**Cuando la identidad falla, la ruta la rescata, y sólo si se puede demostrar que el archivo anterior ha dejado de existir.**

Un candidato se reconcilia con la entidad que ya ocupaba su ruta si se cumplen las **tres** condiciones a la vez:

1. Su `(volumen, FileID)` no se conocía.
2. Su ruta la reclama **exactamente una** entidad viva.
3. El `FileID` de esa entidad **no aparece en ningún sitio de este recorrido**.

Al reconciliar, el `file_id` caducado se **retracta** (I4: no se borra) y se afirma el nuevo. La entidad es la misma, así que conserva su historial, sus aristas, su clasificación de sensibilidad y su fila en `extractions` —que, al ver un `file_hash` distinto, reextrae y **sustituye** sus fragmentos, como ya hacía—.

### La tercera condición es la decisión entera

Las dos primeras describen el síntoma. La tercera es lo que separa una regla determinista de una heurística: si el archivo de antes siguiera existiendo —movido a otra carpeta—, su `FileID` estaría entre los candidatos del recorrido, y entonces lo que hay ahora en la ruta antigua es genuinamente otro archivo. Sin esa comprobación, mover un documento y crear otro con su nombre fusionaría dos historias sin relación, que es un error bastante peor que el que se está arreglando: el fantasma **duplica** conocimiento, la fusión errónea lo **corrompe**.

Cuesta un `HashSet` de las identidades del recorrido, construido antes de escribir nada.

### La regla no exige que el hash coincida, a propósito

La formulación intuitiva es «misma ruta **y** mismo contenido, luego es el mismo archivo». Es más segura y **habría dejado fuera justo el caso que destapó el problema**: `search.rs` cambió de `FileID` porque se editó, así que su contenido también cambió. Nadie guarda un archivo sin modificarlo; exigir el hash reconciliaría sólo las copias de seguridad y los `touch`.

El hash entra en el informe como matiz —¿mismo contenido o edición?— y no como condición.

## Alternativas consideradas

### A. Reconciliar por `(ruta + hash)`, sin más

Lo que recomendaba el informe de la puerta. Rechazada por lo de arriba: cubre el caso raro y deja fuera el común. Se conserva su intención —la ruta es la señal de rescate— y se sustituye el hash por una prueba de que el archivo viejo desapareció, que es lo que el hash intentaba aproximar.

### B. No fusionar: crear la entidad nueva y anotar `supersedes` hacia la vieja

Coherente con «nada se borra» y con el vocabulario de §3.4, que ya tiene el predicado. Rechazada porque no resuelve el problema que lo motivó: el fantasma sigue teniendo sus fragmentos y sus vectores, y la búsqueda semántica sigue devolviendo el documento dos veces. Habría hecho falta, además, que toda consulta supiera seguir cadenas de `supersedes` hacia adelante — es decir, mover el coste a los cientos de sitios que leen el grafo en vez de resolverlo en el único que escribe.

Sigue siendo la opción correcta el día que el sistema quiera ofrecer «ver una versión anterior de este documento», y entonces se construye **sobre** la entidad única, con la bitemporalidad que el log ya tiene.

### C. Preguntar al sistema de archivos si el `FileID` viejo sigue existiendo (`OpenFileById`)

La prueba directa en vez de la indirecta, y contesta también por los archivos que se movieron fuera de la raíz del escaneo. Rechazada por coste/beneficio: es una llamada Win32 más, exige un handle del volumen, y sólo mejora un caso —el archivo se movió fuera de lo escaneado **y** otro ocupó su ruta exacta— que es raro y cuyo fallo es el mismo que había antes de este ADR. Si aparece, se revisa.

### D. Cambiar la identidad a `(volumen, ruta)` y dejar de usar el `FileID`

La rendición. Rechazada porque rompe lo que I8 sí cumple: mover y renombrar dejarían de conservar el conocimiento acumulado, que es el caso frecuente de verdad. El `FileID` acierta casi siempre; el problema es el «casi».

### E. Usar el USN Change Journal (§4.1) para ver el renombrado

La solución correcta a largo plazo: el journal registra el `RENAME_NEW_NAME` y permite seguir la identidad sin adivinar. Rechazada **para ahora** porque el journal no está implementado —M1 lo dejó anotado como pendiente para la vigilancia continua— y porque no ayuda al escaneo completo, que es lo que hay hoy. Cuando exista, esta regla pasa a ser el camino de reserva para cuando el journal se haya truncado o el volumen se haya montado en otra máquina.

## Consecuencias

**A favor**

- **El índice vectorial no ve el mismo documento dos veces.** Era el motivo de hacerlo ahora.
- **El conocimiento sobrevive a guardar.** La clasificación de sensibilidad, las aristas y el historial de un documento ya no se pierden al pulsar Ctrl+S, que era una pérdida silenciosa y frecuente.
- **La reconciliación no vive en el log.** A la tercera pasada el archivo se reconoce ya por identidad (`entities_recognised`, no `entities_reconciled`): arregla el estado y desaparece.
- **Se mide.** `entities_reconciled` y `assertions_retracted` salen en el informe del escaneo. Un número que sube sin parar sería la señal de que el `FileID` no vale en algún volumen concreto.
- **La idempotencia de M1 se conserva.** Sin cambios en el disco: 0 escrituras, 0 reconciliaciones. Hay un test.

**En contra**

- **Es la primera vez que la ruta influye en la identidad.** I8 decía que la ruta es una observación; ahora es una observación que, en un caso acotado, decide. Está acotado y demostrado, pero la frase del invariante ya no es literalmente cierta y este ADR es donde consta.
- **`obrain-scan` retracta.** Hasta ahora sólo lo hacía la puerta. Retracta exclusivamente aserciones `file_id` que él mismo escribió y que se refieren a un archivo que ha demostrado no existir; aun así, es una capacidad nueva en el escáner y conviene que esté escrita.
- **Un archivo borrado y sustituido por otro distinto con el mismo nombre se fusiona.** Es el falso positivo que queda: si borras `notas.txt` y creas otro `notas.txt` sin relación, hereda la historia del anterior. No se distingue del guardado atómico sin preguntar al sistema de archivos (alternativa C). Se considera aceptable porque el nombre reutilizado suele indicar continuidad de propósito, y porque el error es corregible: I5 permite al usuario contradecirlo.
- **No arregla las entidades fantasma que ya estén en una base.** La reconciliación actúa cuando el archivo se vuelve a ver; un fantasma cuya ruta ya no existe se queda. Limpiarlos es un mantenimiento aparte que todavía no existe.
- **`path_observed_at` sigue acumulando observaciones vivas.** Un archivo movido conserva viva la ruta antigua, así que la consulta «rutas con más de una entidad viva» puede contar un falso positivo si otro archivo ocupa después esa ruta. Lo correcto sería cerrar la observación con `valid_to`, que el modelo ya soporta y el escáner no usa. Queda anotado.
