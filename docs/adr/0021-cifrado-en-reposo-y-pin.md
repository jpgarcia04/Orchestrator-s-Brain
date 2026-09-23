# ADR-0021 — Cifrado en reposo: toda la base con SQLCipher, la llave en Windows y un PIN para ver

- **Estado:** Aceptada
- **Fecha:** 2026-09-22
- **Decide:** la pregunta del PIN de §10 del maestro, y §8.1 («en reposo: SQLCipher»)
- **Depende de:** [ADR-0006](0006-donde-vive-la-sensibilidad.md), [ADR-0020](0020-sensibilidad-por-carpeta.md)

## Contexto

La base (`%LOCALAPPDATA%\obrain\brain.obrain`; desde [ADR-0029](0029-una-carpeta-fija-fuera-de-appdata.md), `%USERPROFILE%\.obrain\brain.obrain`) es un SQLite **sin cifrar**. Con
una bóveda de documentos personales eso significa que el texto extraído del
pasaporte, de la póliza y del contrato, sus nombres, sus rutas y las marcas del
usuario están en claro en un archivo que cualquiera puede copiar. El usuario lo
puso como condición: *«tiene que estar antes de que meta mi pasaporte de
verdad»*.

La decisión se le presentó **antes** de escribir código, con cuatro preguntas y
sus costes, y eligió en las cuatro la opción recomendada (2026-09-22):

| Pregunta | Eligió |
|---|---|
| ¿Qué se cifra? | **Toda la base** (SQLCipher) |
| ¿Dónde vive la llave? | **Windows (DPAPI)** |
| ¿Qué protege el PIN? | **Ver**, no descifrar |
| ¿Y si se pierde la llave? | **Una clave de recuperación** |

Antes de elegir se le dijeron dos cosas que acotan todo lo demás:

1. **Cifrar la base no cifra los originales.** El PDF del pasaporte sigue en su
   carpeta. Contra un robo de la laptop apagada, la protección de verdad es el
   cifrado de disco de Windows (Cifrado de dispositivo / BitLocker).
2. **La base es reconstruible** (I1). Perder la llave no pierde documentos:
   pierde las marcas, las correcciones y el historial, y el resto se vuelve a
   indexar.

### Lo que protege y lo que no

| Amenaza | ¿La cubre? |
|---|---|
| Alguien copia `brain.obrain` (una copia de seguridad, un USB, otra cuenta de Windows) | **Sí.** Sin la llave es ruido: *file is not a database*. |
| Alguien usa tu sesión de Windows abierta y abre la aplicación | **A medias:** el PIN tapa el contenido sensible dentro de la app. |
| Alguien usa tu sesión abierta y abre la carpeta de originales | **No.** Eso es el sistema de archivos, no obrain. |
| Malware ejecutándose como tú | **No.** DPAPI descifra para tu usuario, sea quien sea el proceso. |
| Robo de la laptop apagada | Sólo la base. Los originales, **sólo con cifrado de disco**. |

## Decisión

### 1 · Toda la base, con SQLCipher 4

`rusqlite` con `bundled-sqlcipher-vendored-openssl`: SQLCipher 4 compilado dentro
del programa, con OpenSSL compilado también dentro. Se cifra **todo** —el log,
los fragmentos de texto, el índice FTS5, los vectores, las marcas— y también el
WAL. La llave es de **256 bits aleatorios** y se entrega en crudo
(`PRAGMA key = "x'…'"`), así que abrir la base no paga una derivación de clave.

### 2 · La llave, envuelta dos veces en `<base>.llave`

Junto a la base vive un archivo pequeño (`brain.obrain.llave`) con la misma
llave guardada **dos veces**, cada una en su caja:

- **La caja del sistema**: la llave cifrada con **DPAPI**, en el ámbito del
  usuario de Windows. Es la que se usa a diario: la app y `obrain indexar` abren
  la base sin preguntar nada. Fuera de Windows —sólo el banco de pruebas— esta
  caja se guarda **sin proteger**, y el archivo lo dice (`"tipo":
  "sin-proteger"`): no es un modo de uso, es la forma de que los tests corran en
  Linux.
- **La caja de recuperación**: la llave cifrada con XChaCha20-Poly1305 bajo una
  clave derivada con **Argon2id** (64 MiB, 3 pasadas) de la **clave de
  recuperación**: 25 caracteres en base32 de Crockford (125 bits), en cinco
  grupos: `K7Q2M-9XP4T-R8…`. Se enseña al crearla y se puede volver a ver desde
  la app mientras la base está abierta; si Windows pierde la llave (otro
  equipo, Windows reinstalado, una contraseña restablecida por un
  administrador), con ella se abre la base y la caja del sistema se rehace.

### 3 · Abrir es transparente; crear y migrar, explícito

- `Store::open` mira si existe `<base>.llave`: si existe, abre con la llave de
  Windows; si no, abre en claro (los tests y las bases en memoria no cambian).
- `obrain_api::cifrado::abrir` es la entrada del producto: si no hay base, la
  crea cifrada; si hay una **en claro**, la **migra** en el sitio
  (`sqlcipher_export` a un archivo nuevo, comprobación, y sustitución), de forma
  que una interrupción a medias se reanuda en la apertura siguiente. La base de
  agosto de JP se cifra sola la primera vez.
- Lo que había en claro **puede seguir en el espacio libre del SSD** hasta que se
  sobrescriba. Borrar de forma segura en un SSD no es posible desde una
  aplicación; eso también lo resuelve el cifrado de disco.

### 4 · El PIN protege **ver**

- De 4 a 8 dígitos. Se guarda su hash **Argon2id** dentro de la base cifrada.
- **Lo impone el núcleo, no la ventana.** Con el PIN puesto y sin desbloquear,
  el contenido de lo sensible **no sale del núcleo**: la vista previa llega sin
  texto ni imagen, los resultados de búsqueda de lo sensible llegan sin su
  fragmento y «Abrir» se niega. Hoy la ventana recibe el contenido y lo tapa con
  un borroso; con PIN, ni lo recibe.
- Un desbloqueo dura **5 minutos desde el último uso**. Tras 3 fallos, espera
  creciente (30 s, 1 min, 2 min… hasta 15 min).
- **Si se olvida**, se restablece con la **clave de recuperación**. Se propuso
  hacerlo con la contraseña de Windows; se hizo así porque validar la contraseña
  de Windows —o Windows Hello— desde una app de escritorio es código que sólo se
  puede probar en Windows real, y la clave de recuperación ya existe y se prueba
  en cada test. Restablecer con Windows Hello queda para cuando se pruebe en la
  laptop de JP.

## Alternativas consideradas

### A. Cifrar sólo lo sensible, dentro de la app

Rust puro, sin instalar nada para compilar. Rechazada por el usuario: los
nombres y las rutas quedarían en claro («Pasaporte.pdf» se ve), el texto de lo
sensible dejaría de ser buscable hasta desbloquear —el índice FTS5 no puede
indexar texto cifrado— y cada lector de la base tendría que saber descifrar.

### B. Una frase de paso al abrir

Protege incluso con la sesión abierta, pero hay que escribirla cada vez —en la
app y en la terminal— y olvidarla sin clave de recuperación es perder la base.
Rechazada por fricción diaria para una laptop de una sola persona.

### C. Windows Hello (TPM) como llave

El TPM limita los intentos contra el PIN de Windows, que es lo que haría seguro
un PIN corto como llave de descifrado. Rechazada **por ahora**: no se puede
probar fuera de Windows y restablecer el PIN de Windows destruye las llaves que
guarda. Es el camino natural si algún día el PIN tiene que proteger descifrar.

### D. SQLite3 Multiple Ciphers en vez de SQLCipher

No necesita OpenSSL ni Perl para compilar. Rechazada por ahora: no tiene un
enlace mantenido con `rusqlite` y habría que compilar su amalgamación a mano en
cada plataforma. Si instalar Perl resulta un problema en Windows, es la primera
alternativa a revisar: lee el formato de SQLCipher.

### E. Sin clave de recuperación

Una llave perdida sería una base nueva y reindexar. Rechazada por el usuario:
perdería sus marcas y correcciones, que son justo lo que no se puede
reconstruir.

## Consecuencias

**A favor**

- Una copia de la base, sin la llave, no dice nada: ni textos, ni nombres, ni
  rutas.
- A diario no cambia nada: la app y la terminal abren solas.
- El PIN deja de ser un gesto: con él puesto, el contenido sensible no llega a
  la ventana hasta desbloquear.

**En contra**

- **Compilar en Windows pide Perl** (Strawberry Perl, una vez) porque OpenSSL se
  compila dentro, y la primera compilación tarda unos minutos más. NASM es
  opcional: sin él, OpenSSL se compila sin ensamblador.
- **La base y su `.llave` van juntas.** Una copia de seguridad de la base sin la
  clave de recuperación no se abre en otro equipo; es lo que se pidió, y hay que
  saberlo antes de necesitarla.
- **El código de DPAPI sólo se ha comprobado que compila** (`cargo check
  --target x86_64-pc-windows-msvc`); que funcione se sabrá en Windows.
- **Nada de esto protege los originales.** Se dice en la app y en `ESTADO.md`.
- SQLCipher cuesta algo de rendimiento (cifrar y autenticar cada página); con
  una bóveda personal no debería notarse.
