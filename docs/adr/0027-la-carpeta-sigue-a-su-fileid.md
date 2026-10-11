# ADR-0027 — La carpeta sigue a su `FileID`: renombrarla no le quita la marca

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Resuelve:** [ADR-0020](0020-sensibilidad-por-carpeta.md), «En contra»: *renombrar o mover la carpeta deja la marca atrás*
- **Depende de:** [ADR-0007](0007-identidad-tras-un-guardado-atomico.md), [ADR-0018](0018-bibliotecas.md), [ADR-0025](0025-lo-que-ya-no-esta.md)

## Contexto

Una carpeta es una entidad del log desde M3, pero **se reconoce por su ruta**:
el enlazador busca `folder_path` y, si no la encuentra, crea otra. Los
archivos no funcionan así desde M1: se reconocen por `(volumen, FileID)` y la
ruta es una observación (I8).

Por eso renombrar `01 Identidad` a `01 Identidad y viajes` deja la marca
de sensible en una carpeta que ya no existe. Lo de dentro **vuelve a su
nivel sin avisar**, y «Sensibles» lo enseña en rojo. [ADR-0020] lo aceptó como coste y
dejó escrita la solución: que la carpeta conserve su identidad al
renombrarse.

Lo que se comprobó antes de decidir (2026-09-23, en el Windows del usuario):

- En NTFS un directorio tiene `FileID` como un archivo, y **no cambia** al
  renombrarlo, al moverlo a otra carpeta del mismo disco ni al renombrar su
  carpeta madre (`fsutil file queryfileid`, antes y después de cada paso).
- `obrain-scan/src/identity.rs` ya abre con `FILE_FLAG_BACKUP_SEMANTICS`, que
  es lo que hace falta para abrir un directorio: la misma función sirve para
  carpetas sin una línea nueva de código de Windows. En Linux sirve el inodo.
- Nadie retracta nunca el `folder_path` de una carpeta, y el árbol cuelga cada
  carpeta de **todas** sus aristas `contained_by` vigentes: una carpeta movida
  a otra madre saldría bajo las dos.

Se le enseñaron al usuario tres opciones con su coste: **A** · apuntar el
`FileID` y pasar sólo la marca a la carpeta nueva; **B** · que la carpeta siga
a su `FileID` como los archivos; **C** · sin `FileID`, que la marca siga a los
documentos que se mudan juntos. Eligió **B** (2026-09-23).

## Decisión

### 1 · Cada carpeta apunta su `FileID`

El escáner ya recorría las carpetas para bajar por ellas; ahora también
devuelve su identidad, con el mismo triaje que un archivo (una carpeta que no
se puede abrir sin descargar no se abre). Tras enlazar, cada carpeta del log
que tiene la ruta de una carpeta recorrida afirma **`folder_id`** («identidad
de carpeta», cardinalidad `one`, determinista, `scan@m1`).

Si en una ruta aparece **otro** `FileID` (la carpeta se borró y se creó otra
con el mismo nombre), la entidad de esa ruta se queda con el nuevo, igual que
un archivo tras un guardado atómico ([ADR-0007]): una carpeta nueva con el
nombre de una marcada sigue marcada, que es lo seguro.

### 2 · Al ver un `FileID` conocido en otra ruta, la carpeta se muda

Antes de enlazar, cada carpeta del log cuyo `folder_id` aparece en el recorrido
bajo **otra ruta** se retracta su `folder_path` viejo y se afirma el nuevo **en
la misma entidad**. El enlazador la encuentra por su ruta nueva y no crea otra.

Todo lo que cuelga de ella viaja solo: **la marca con su fecha original** (la
regla de «la última palabra del usuario» de [ADR-0020] sigue intacta), su
estrella en el mapa, la faceta de proyecto y la historia. *«¿Qué protegía en
marzo?»* sigue dando la ruta de marzo, porque la ruta vieja se retracta y no se
borra (I4).

Renombrar la carpeta madre cambia la ruta de todas las de dentro. Cada una
tiene su `FileID` y se muda por su cuenta.

### 3 · El enlazador suelta la madre vieja

Una carpeta que cuelga de otra madre retracta su `contained_by` viejo, como ya
hacía un archivo movido ([ADR-0025]). Sólo lo que escribió el enlazador.

### 4 · Seguros

Mudarse de más es peor que no mudarse: en la duda, la carpeta se queda donde
estaba y «Sensibles» sigue avisando en rojo.

1. **La ruta vieja tiene que haber desaparecido.** Se muda si el disco dice
   *no encontrado* en la ruta vieja o si el recorrido vio allí otra carpeta.
   Cualquier otra respuesta del disco la deja quieta. Cambiar sólo mayúsculas
   (`01 identidad` → `01 Identidad`) también cuenta como mudanza: en Windows la
   ruta vieja «existe» porque es la misma.
2. **Una biblioteca no se muda.** Es la ruta que eligió el usuario
   ([ADR-0018]); si la mueve, falta entera, y eso ya tiene su regla.
3. **Si la ruta nueva ya la tiene otra carpeta del log** que no se muda —una
   que se borró hace meses, con el mismo nombre—, ésa se retira (se retracta
   su ruta) y la que llega ocupa su sitio. **Salvo que la vieja tenga una
   marca o sea una biblioteca**: entonces no se toca nada y la marca vieja
   sigue cubriendo esa ruta como hasta hoy. No se fusionan entidades, y no se
   copian marcas de una entidad a otra (eso habría sido la opción A, con la
   fecha de la marca cambiada).

### 5 · Lo que significa para el usuario

**La marca es de la carpeta, no de su nombre.** Si renombras `01 Identidad` a
`Viejo` y creas otra `01 Identidad` vacía, la marca se va con `Viejo`: es el
cajón con su llave, no la etiqueta del cajón. Lo de dentro de la carpeta nueva
se clasifica como siempre, por su nombre y su texto.

En la vista «Sensibles», una carpeta marcada vacía ya no pregunta *«¿la
renombraste o la moviste?»*, porque eso ya no la vacía. Pregunta *«¿la
borraste, o la moviste a otro disco?»*.

## Alternativas consideradas

### A. Apuntar el `FileID` y pasar sólo la marca

La carpeta renombrada sería una entidad nueva, y la marca se retractaría en la
vieja y se afirmaría en la nueva. Cuesta lo mismo en el escáner que B.
Rechazada: la marca copiada nace con fecha nueva y rompe la regla 4 de
[ADR-0020]. Un archivo que dijiste «normal» después de marcar la carpeta, pero
antes de renombrarla, volvería a subir. Además, sólo viajaría la marca:
la historia de la carpeta se partiría en dos entidades.

### C. Sin `FileID`: la marca sigue a los documentos

Si todos los documentos de una carpeta marcada reaparecen juntos bajo otra
ruta, la marca se va con ellos. Funciona en un USB en FAT. Rechazada: es una
heurística. Una carpeta vacía, o una de la que sólo moviste una parte, pierde
la marca, y en la bóveda del usuario (NTFS) no aporta nada que B no dé con
certeza.

### D. Que la carpeta se identifique **sólo** por `FileID`

Crear las carpetas por `FileID` en el enlazador y no por ruta. Rechazada por
innecesaria: las carpetas que el enlazador conoce son exactamente las que el
escáner recorre, y basta con poner la ruta al día antes de enlazar. Además, la
ruta sigue siendo la llave que usan la terminal (`obrain sensible <carpeta>`)
y las bibliotecas.

## Consecuencias

**A favor**

- Renombrar o mover una carpeta marcada dentro del mismo disco **ya no la
  desprotege**: lo de dentro sigue sensible sin tocar nada.
- La carpeta conserva su sitio en el mapa y su historia.
- Sin código nuevo de Windows: la identidad es la misma función que usan los
  archivos, y se ha probado en NTFS de verdad, no sólo compilado.

**En contra**

- **Mover una carpeta a otro disco** (un USB, otra partición) le cambia el
  `FileID`: ahí la marca se queda en la ruta vieja, como antes, y «Sensibles»
  avisa en rojo.
- **Los renombrados de antes de esta versión no se recuperan.** Esas carpetas
  no tenían `FileID` apuntado; la marca huérfana sigue saliendo en rojo y basta
  con volver a marcar la carpeta.
- Hace falta que la carpeta se haya visto **una vez** con esta versión antes
  de renombrarla: el `FileID` se apunta al indexar.
- El escáner abre un handle por carpeta, sólo de atributos. En una bóveda
  personal son decenas; no se nota.
- En Linux —el banco de pruebas— un inodo de una carpeta borrada puede
  reutilizarse, y una carpeta nueva heredaría la entidad de la borrada. Sólo
  pasa si la ruta vieja ya no existe, y el error va hacia el lado de proteger
  de más. En NTFS el `FileID` lleva un número de secuencia y eso no pasa.

[ADR-0007]: 0007-identidad-tras-un-guardado-atomico.md
[ADR-0018]: 0018-bibliotecas.md
[ADR-0020]: 0020-sensibilidad-por-carpeta.md
[ADR-0025]: 0025-lo-que-ya-no-esta.md
