# ADR-0030 — Un documento dentro de otro

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Resuelve:** la pregunta abierta del maestro *«¿Cómo se representa que un documento está dentro de otro?»* (§10), aparecida al auditar la prueba ([ADR-0016](0016-la-prueba-se-elige-el-indice-no-se-filtra.md))
- **Toca:** [ADR-0020](0020-sensibilidad-por-carpeta.md) (de dónde sale la sensibilidad), [ADR-0005](0005-donde-vive-el-texto-extraido.md) (el texto del que se lee)

## Contexto

En el corpus real, `Estados_de_Cuenta_Abuelo_Visa.pdf` **contiene** los
estados de cuenta mensuales del banco que también están sueltos, y
`Documentos_Identidad_Respaldo_Visa.pdf` contiene otro. Son los PDF que se
arman para un trámite juntando otros PDF. Hoy obrain no tiene forma de decirlo:
M3 modela `duplicate_of` para archivos idénticos, pero no *«este documento es
parte de aquel»*. Sin esa relación, una búsqueda devuelve cinco filas donde hay
tres documentos, sin poder explicar por qué se parecen tanto.

El usuario decidió el planteamiento el 2026-09-23 (maestro §10): **un predicado
nuevo, detectado sin IA comparando hashes de fragmento**. La vista previa dice
qué contiene y la búsqueda agrupa las filas diciendo por qué, **sin esconder
ninguna**.

### Lo que se midió antes de programar

Se midió sobre una copia de la base de agosto del usuario (561 documentos con
texto, 4.533 fragmentos), enseñando sólo cuentas y nombres enmascarados.

**Por fragmentos no funciona.** El chunker (`paragraph-pack@1`) junta párrafos
hasta 1.200 bytes y solapa 200 con el anterior. Dentro del PDF grande, el
primer y el último fragmento de cada documento se mezclan con el texto del de
al lado, así que nunca son idénticos. De los 14 casos reales que hay en esa
base, la comparación por fragmentos (≥ 95 % idénticos) encuentra **uno**. A
los documentos de una sola página (1,3 KB) no les coincide ni un fragmento.

**Por párrafos sí.** Un párrafo (lo que separan las líneas en blanco, que es
la unidad con la que el chunker ya trabaja) sale igual en el documento suelto y
dentro del paquete. Con párrafos se encuentran los 14.

**Lo difícil es no confundir un paquete con una versión.** En esa base hay
documentos que son *casi* otro: un formulario y su copia con un párrafo
cambiado, `presentación (1).pptx` con una línea más, un PDF de 718 bytes y otro
de 721. Todos tienen «el 95 % de B está en A». Lo que los separa se vio al
mirar **dónde** está lo que no casa:

| | Paquetes (14) | Versiones |
|---|---|---|
| Lo de B que no está en A | 0–5 %, **sólo en la orilla** (el primer o el último párrafo, pegado al vecino) | hasta 8 %, **en medio** |
| Cuánto de A es B | 3–87 % | 92–100 % |

Un documento dentro de un paquete aparece **de un tirón**: sus párrafos
seguidos, en el mismo orden, sin nada intercalado. Una versión tiene cambios
repartidos, o es el mismo documento con algo añadido al final.

Otros parecidos que la regla no debe tomar: un `package-lock.json` dentro de
otro (81 %, dependencias sueltas), las páginas HTML de un sitio que comparten
su menú (69 %), y el texto legal que repiten todos los estados de cuenta.

## Decisión

### 1 · El predicado: `part_of` («está dentro de»)

- **B `part_of` A**: el texto de B está entero dentro del de A. Sujeto el de
  dentro, objeto el de fuera, como `contained_by` (archivo → carpeta).
- Cardinalidad `many`: un estado de cuenta puede estar en dos paquetes.
- Su inverso, `has_part` («contiene»), se declara pero **no se almacena**
  (§3.4: una arista se guarda una vez).
- No se reutiliza `contains`/`contained_by`: son el árbol de carpetas, y
  mezclarlos haría que el mapa dibujara un PDF como si fuera una carpeta.
- Método `deterministic`: comparar huellas no adivina nada. Confianza = la
  parte de B que aparece de un tirón (0,95–1,00).

### 2 · Cómo se detecta: párrafos idénticos, de un tirón

Lo hace `obrain-link` (M3, «relacionar sin IA») en una etapa nueva que corre
**después de leer el texto** (M2) y **antes de la puerta**, para que la
sensibilidad ya la vea (§4).

1. **Párrafos.** El texto de cada documento se parte como lo parte el chunker
   (líneas en blanco), se normalizan los espacios y se saca la huella (BLAKE3)
   de cada párrafo de **40 bytes o más**. Los más cortos («Página 3 de 20», una
   fecha suelta) cambian de un PDF a otro y no dicen nada; no cuentan ni para
   bien ni para mal.
2. **Candidatos.** Con un índice huella → documentos, se busca cada A que tenga
   al menos el **95 %** de los bytes de B.
3. **B está dentro de A si:**
   - **aparece de un tirón**: el tramo más largo de párrafos de B que están
     seguidos y en el mismo orden en A cubre al menos el **95 %** de B. Lo que
     sobre sólo puede estar en las orillas;
   - **A trae algo más**: B es como mucho el **90 %** de A. Si no, es la misma
     cosa con un añadido (una versión), y las versiones siguen congeladas;
   - **B tiene al menos 3 párrafos**: un documento de una línea «cabe» en
     cualquier sitio.
4. **Textos idénticos, una vez.** De cada grupo de documentos con el mismo
   texto (copias, el mismo PDF guardado otra vez) sólo cuenta uno, el más
   antiguo, que es el criterio de `duplicate_of`. Así un paquete no dice
   «contiene: B, B, B, B», y una copia llega a él por su «Duplicado de».
5. **La evidencia** dice dónde está: `parrafos=89/89 bytes=1234..15678`. Los
   bytes son **del texto de A**, que es donde el «¿por qué?» lo marca. Como
   siempre, ni una palabra del documento va al log. El tramo incluye los
   párrafos cortos de las orillas de B —el título «MOVIMIENTOS DE MARZO», una
   línea de cierre— si están igual y seguidos en A: no cuentan para decidir,
   pero sin ellos lo marcado empezaba en la segunda línea (visto en el banco
   de pruebas).
6. **Sigue al texto.** Cada pasada recalcula todo, igual que los vencimientos.
   Una arista que el texto de hoy ya no sostiene se retracta, y una que cambia
   de tramo se retracta y se vuelve a afirmar. Lo que está en la papelera no
   se toca mientras esté ahí (vuelve con lo que sabía, ADR-0025), y lo olvidado
   se retracta. Fuente: `linker:parrafos@v1`.

Sobre la base de agosto: **14 relaciones, cero falsas**. Las 4 versiones, el
`package-lock`, el HTML y el texto legal quedan fuera.

### 3 · Lo que se ve

- **Vista previa del paquete:** un apartado **«Contiene (N)»**, plegable y
  recordado como los otros, con cada documento. Su **«¿por qué?»** dice *«Sus
  89 párrafos están aquí, seguidos y en el mismo orden»* y marca en el texto
  del paquete dónde empieza y dónde acaba. Un clic en el nombre lo abre.
- **Vista previa del documento suelto:** en «Relacionado», **«Está dentro de
  Estados_de_Cuenta_Abuelo_Visa.pdf»**. Un clic abre el paquete **por ese
  tramo, marcado**. La vista previa acepta ahora un tramo además de la
  consulta, porque un paquete de 80 KB no cabe entero en los 40 KB que viajan
  a la ventana.
- **Búsqueda** (elegido por el usuario): cuando salen el paquete y un documento
  que lleva dentro, **el de dentro va justo debajo del paquete**, un poco
  metido, con «está dentro de…». Ninguna fila se esconde, y el grupo sube al
  puesto del mejor de los dos.
- **Mapa:** la lente ya dibuja cualquier arista entre documentos; la línea
  aparece sola.
- **Terminal:** `obrain paquetes` lista qué contiene qué.

### 4 · El de fuera hereda la sensibilidad (elegido por el usuario)

Un paquete es, como poco, tan sensible como lo más sensible que lleva dentro.
El caso: el pasaporte está en `01 Identidad`, una carpeta marcada, y el PDF de
la visa en `04 Trámites` lo lleva dentro. obrain no tiene una regla que
reconozca un pasaporte mexicano por su texto, así que ese PDF saldría **sin
tapar**, en la búsqueda y en la vista previa, y sin pedir el PIN.

- Vive en la misma regla que la herencia por carpeta
  (`Store::resolve_sensitivity`, ADR-0020): **un suelo, no un nivel**. Sube el
  paquete y nunca lo baja.
- Cuenta el nivel **propio** de cada documento de dentro: su clasificador, su
  carpeta marcada o lo que dijiste tú. No hace falta seguir la cadena: si C está
  en B y B en A, el texto de C también está en A y hay arista directa.
- **Lo que tú digas del paquete gana.** Si lo miraste y dijiste «normal», es una
  excepción consciente, como con las carpetas.
- A igual nivel se explica por lo propio del paquete. Sólo cuando lo de dentro
  sube el nivel, el «¿por qué?» dice *«contiene «pasaporte.pdf», que es
  sensible por tu carpeta «01 Identidad»»*, con un clic hacia él.
- Cuando cambia lo de dentro (marcas su carpeta, corriges su nivel), el paquete
  se recalcula en el acto, no en la pasada siguiente.
- La puerta, el PIN y la búsqueda no cambian: leen la columna de siempre.

Si alguna vez una relación fuera falsa, lo único que pasa es que tapa de más.
Es el lado hacia el que se equivoca un control de seguridad.

## Alternativas consideradas

### A. Por fragmentos, como se decidió al principio

Igual de barato, y es lo que decidió el usuario antes de medir. Rechazada con
el número delante (1 de 14), y el usuario eligió párrafos al verlo.

### B. Texto parecido (tejas de palabras, *shingles*)

Toleraría el ruido del OCR: una foto del pasaporte dentro de un PDF escaneado.
Rechazada por ahora: más lenta, y con el texto legal repetido de los estados de
cuenta daría falsas alarmas que hoy no hay. Si un día hace falta, se mide igual
que ésta.

### C. Guardar las huellas de párrafo en una tabla

Evitaría releer todo el texto en cada pasada. No hace falta todavía: sobre la
base de agosto todo el cálculo son décimas de segundo, y el guardián y los
vencimientos ya releen el texto en cada pasada. Se hará cuando se mida lento.

### D. Sensibilidad: sólo avisar, o nada

Se le ofrecieron al usuario. Rechazadas: el paquete saldría sin tapar, que es
lo que no puede pasar con un documento de identidad dentro.

### E. En la búsqueda, dejar cada fila en su sitio con una nota

Más sencillo, pero el paquete y lo que contiene pueden salir separados, y la
razón de que se parezcan queda repartida. El usuario prefirió agrupar.

### F. Detectar también las versiones

Salen casi solas de la misma medida (los «casi iguales» de arriba). No se
hace: la detección de versiones está congelada. Aquí sólo se cuidan para no
confundirlas con un paquete.

## Consecuencias

**A favor**

- Los paquetes de un trámite dicen lo que llevan, y cada documento suelto sabe
  en qué paquete está.
- La búsqueda deja de enseñar cinco filas sin explicar por qué se parecen: el
  paquete y lo suyo salen juntos, y se dice por qué.
- Un documento de identidad dentro de un paquete protege al paquete.
- Todo es comprobable: la evidencia dice qué párrafos y dónde, y el «¿por qué?»
  lo marca.

**En contra**

- **Sólo ve lo que se leyó igual.** Una foto dentro de un PDF escaneado, o un
  PDF que se volvió a escanear, no se detecta: el OCR nunca lee dos veces
  igual. Sirve para los paquetes que se arman juntando PDF que ya traen texto,
  que son los del corpus real.
- **Un paquete que retoca lo que junta** (cambia la cabecera de cada página a
  media hoja, reordena párrafos) se le escapa: la regla pide el documento de un
  tirón. Se prefirió eso a confundir versiones.
- **Un paquete que es casi todo un documento** (B es más del 90 % de A) se toma
  por una versión y no se dice nada. El caso real más justo es el 87 %.
- **Cada pasada relee el texto de todo**, como el guardián y los vencimientos.
  Hoy cuesta décimas de segundo; en una bóveda diez veces mayor habrá que
  guardar las huellas (alternativa C).
- **Una copia idéntica** de un documento que está en un paquete no dice «está
  dentro de»; lo dice su original, al que llega por «Duplicado de».
