# ADR-0025 — Lo que ya no está: una papelera de 30 días, y el log lo recuerda

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Decide:** la primera pregunta abierta del maestro (§10: *¿el conocimiento sobrevive al archivo que lo originó?*), para los documentos de una biblioteca
- **Depende de:** [ADR-0001](0001-log-de-aserciones-como-fuente-de-verdad.md), [ADR-0005](0005-donde-vive-el-texto-extraido.md), [ADR-0007](0007-identidad-tras-un-guardado-atomico.md), [ADR-0018](0018-bibliotecas.md), [ADR-0024](0024-vigilar-la-carpeta-en-vivo.md)

## Contexto

Desde M1 el escáner apunta lo que ve y **nunca lo que deja de ver**. Con la
vigilancia en vivo (ADR-0024) se nota enseguida: un experimento con el
pipeline real sobre una bóveda de prueba dio esto:

| Qué se hizo en el disco | Qué enseñaba la app |
|---|---|
| Borrar `pasaporte.txt` | Seguía en Recientes, en «Caduca pronto», en su carpeta y en la búsqueda |
| Mover `receta.txt` de `02 Salud` a `01 Identidad` | **Salía en las dos carpetas** |
| Renombrar `02 Salud` a `02 Salud y seguros` | **Salían las dos carpetas**, la vieja con sus documentos |

Los tres tienen la misma causa: una ruta observada (`path_observed_at`) y la
carpeta de un archivo (`contained_by`) se afirman y **no se retractan nunca**.
Lo de mover y renombrar es un error sin nada que decidir. Lo de borrar choca
con la pregunta del maestro (§10), cuya postura provisional era *«sí: el nodo es
conocimiento sobre el archivo, no el archivo»*.

Se le enseñaron al usuario las opciones con su coste antes de escribir código:
que desaparezca de todo y lo recuerde el log; que salga de las listas pero se
siga encontrando buscando; o una vista con lo que desapareció durante 30 días.
Y, aparte, si distinguir *por qué* ya no está (borrado, en la Papelera de
Windows, movido fuera de la bóveda, USB desconectado) preguntándole al disco
por su `FileID`. Eligió (2026-09-23):

| Pregunta | Eligió |
|---|---|
| ¿Qué pasa con lo que borras? | *«Me late tener una papelera: lo que borres se va ahí y se queda 30 días, y si lo quieres borrar inmediatamente que se pueda también, así no saturamos la base de datos»* |
| ¿Distinguir por qué ya no está? | **No**: sólo se mira dentro de tus carpetas |

## Decisión

### 1 · Un documento está en tu bóveda si tiene una ruta vigente

Al terminar de recorrer una biblioteca, lo que el log cree que hay dentro y el
recorrido no vio **se comprueba en el disco** y, si de verdad no existe, pasa a
la papelera:

- se **retractan** su ruta y su carpeta (I4: el log conserva dónde estuvo y
  hasta cuándo);
- se afirma **`missing_since`** («ya no está desde»), con la hora en que el
  sistema se enteró. Determinista, `scan@m1`.

Dos seguros, porque retractar de más es peor que tardar en retractar:

1. **Una biblioteca que falta entera no retracta nada.** Un USB desconectado o
   una carpeta de biblioteca movida no es «se borró todo»: se queda como está,
   igual que hasta hoy.
2. **Sólo cuenta el «no existe» del disco.** Antes de retractar se pregunta por
   la ruta; un error de permisos, un disco que no responde o cualquier cosa que
   no sea *no encontrado* deja el documento donde estaba. Y como la pregunta se
   hace **después** de recorrer y hashear, el hueco de milisegundos de un
   guardado atómico (Word borra y renombra) ya se ha cerrado.

### 2 · Mover y renombrar ya no dejan fantasmas

- Un archivo reconocido en **otra ruta** (por su `FileID`, I8, o por la regla
  de ADR-0007) retracta las rutas que tenía antes.
- Al enlazar, un archivo que cambió de carpeta retracta su `contained_by`
  viejo.
- Una carpeta que se queda **sin nada dentro** no se enseña, igual que ya no se
  enseñaban las raíces vacías. Una carpeta renombrada se queda vacía en su
  nombre viejo, así que desaparece sola. (Una carpeta vacía de verdad tampoco
  salía nunca: el árbol se construye con las carpetas de los archivos.)

### 3 · La papelera

Lo que está en la papelera **sale de todo lo demás**: Recientes, «Caduca
pronto», el explorador, el mapa, la búsqueda, la lente y los «Relacionado» de
otro documento. Y aparece en una vista propia, **«Papelera»**, en la barra
lateral debajo de «Sensibles» (con su cuenta sólo si hay algo):

- cada documento con su nombre, la carpeta donde estaba, cuándo desapareció y
  **cuántos días le quedan** («se olvida en 23 días»);
- un clic lo enseña en la vista previa con lo que se leyó de él —el PIN manda
  igual que siempre—, con el aviso *«Ya no está en tu bóveda»* y sin «Abrir» ni
  «En su carpeta», porque ya no hay archivo;
- **«Olvidar ya»** en cada uno, y **«Vaciar papelera»** para todos (dos clics,
  como quitar la marca de una carpeta).

La **sensibilidad se queda como estaba** al desaparecer: un pasaporte borrado
de `01 Identidad` sigue siendo sensible en la papelera, aunque ya no esté
dentro de la carpeta marcada.

### 4 · Olvidar: el texto se va, el log se queda

A los **30 días** (`PAPELERA_DIAS`, contados desde que obrain se enteró) o
cuando lo pidas, se **olvida** lo que obrain leyó del documento: su texto, sus
fragmentos del índice de palabras, sus vectores y su registro de lectura. Son
**proyecciones** (ADR-0005): se pueden borrar sin tocar la verdad, y son lo que
pesa —el texto y 1,5 KB de vector por fragmento—. Se afirma **`forgotten_at`**
(«olvidado el»), del usuario si lo pediste y determinista si pasó el plazo.

Lo que **se queda en el log** son unas pocas líneas por documento: qué era, dónde
estuvo y hasta cuándo, cuándo caducaba, su sensibilidad y tus correcciones.
Nada se borra del log (I4), y es lo que responde *«¿qué papeles tenía en
marzo?»*.

La etapa de vencimientos **no toca lo que está en la papelera ni lo olvidado**:
sin esto, a la pasada siguiente de olvidar vería un documento sin texto y
retractaría su fecha.

### 5 · Si vuelve, es el mismo

Un documento que reaparece en una biblioteca **sale de la papelera solo**, con
todo lo que se sabía —tus correcciones incluidas— y se retractan su
`missing_since` y su `forgotten_at`. Es el mismo documento si:

- tiene el **mismo `FileID`** (lo sacaste de la Papelera de Windows, que en el
  mismo disco conserva la identidad del archivo), o
- tiene **el mismo contenido** (hash idéntico) que **exactamente uno** de los
  que están en la papelera o se olvidaron (lo copiaste de una copia de
  seguridad).

Si se había olvidado, se vuelve a leer (y a pasar por el OCR, si es una foto).

### 6 · obrain no toca tus archivos

La papelera de obrain **no es la de Windows**. «Olvidar» y «Vaciar papelera» no
borran nada de tu disco —el archivo ya lo borraste tú—: borran lo que obrain
leyó de él. La vista lo dice con esas palabras.

### 7 · Desde la terminal

`obrain papelera` lista lo mismo que la vista, y `obrain papelera --vaciar`
olvida todo ya.

## Alternativas consideradas

### A. Desaparece de todo, sin papelera

Lo más simple: se retracta la ruta y se olvida el texto al momento. Rechazada
por el usuario: un borrado por error no se podría ver ni un momento.

### B. Fuera de las listas, pero se sigue encontrando buscando

Es la postura del maestro al pie de la letra. Rechazada: el texto de lo borrado
—el NIE de un pasaporte viejo— se quedaría en la base para siempre, y la
búsqueda se iría llenando de versiones viejas.

### C. Decir *por qué* ya no está, preguntándole a NTFS por su `FileID`

`OpenFileById` dice si el archivo sigue en el mismo disco y dónde: «está en la
Papelera», «lo moviste a Escritorio». Rechazada por el usuario: obrain miraría
fuera de las carpetas que elegiste, sólo funciona en el mismo disco NTFS y aquí
sólo se podría comprobar que compila.

### D. Olvidar también en el log

Borrar las aserciones del documento a los 30 días. Rechazada: rompe I4, y no es
lo que pesa —unas pocas líneas por documento frente a su texto y sus vectores—.

## Consecuencias

**A favor**

- Lo que borras desaparece de todas partes a los tres segundos (con la app
  abierta) y se puede recuperar de la vista durante 30 días.
- Mover y renombrar dejan de duplicar documentos y carpetas.
- La base no crece con lo borrado: a los 30 días, o al vaciar, sólo quedan unas
  líneas del log por documento.
- Sacar un archivo de la Papelera de Windows lo devuelve con todo lo que sabías.

**En contra**

- **Un archivo que sigue en el disco pero que el escáner ya no recorre** (lo
  metes dentro de una carpeta que se poda, como `node_modules`) no pasa a la
  papelera: el disco dice que existe. Se queda como estaba hasta que lo borres.
- Los 30 días cuentan **desde que obrain se entera**. Con la app cerrada no se
  entera nadie: si borras algo y no abres la app en un mes, empieza a contar al
  abrirla.
- En Linux —el banco de pruebas— el sistema reutiliza los números de inodo, y un
  archivo nuevo puede heredar el de uno borrado y reconocerse como él. En NTFS
  el `FileID` lleva un número de secuencia y eso no pasa.
- Un documento que vuelve después de olvidarse se relee entero, OCR incluido.
- Si marcas una carpeta como sensible **después** de que algo suyo se fuera a la
  papelera, lo de la papelera no cambia: se queda con la sensibilidad que tenía.
