# ADR-0029 — Una carpeta fija fuera de AppData: `%USERPROFILE%\.obrain`

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Resuelve:** el «fallo abierto» del cifrado a medias en Windows (prompt `despues-de-la-carpeta.md`, punto 0)
- **Toca:** [ADR-0021](0021-cifrado-en-reposo-y-pin.md) (dónde vive la base), [ADR-0024](0024-vigilar-la-carpeta-en-vivo.md) (§6: la base no está en tus carpetas)

## Contexto

El 2026-09-23, al abrir la app por primera vez en el Windows del usuario,
`%LOCALAPPDATA%\obrain` pareció quedar con **dos archivos llamados
`brain.obrain`**: la base de agosto en claro, que era la que se veía por la
ruta y la que abría la orden `obrain` («la base no se abre con esta llave»), y
una cifrada que usaba la app y que sólo «apareció» al mover la otra. Se apuntó
como un cifrado a medias, con la sospecha puesta en el `rename` de
`seguridad::reanudar`.

Se investigó sin tocar la base real (con `OBRAIN_DB` y copias del respaldo):

1. El ejecutable de la app y `tauri dev` cifran bien una copia de la base de
   agosto: la copia cifrada sustituye a la original, sin duplicados.
2. La base «cifrada» de las 18:03 **no tiene ni una aserción de agosto**
   (146 entidades en agosto; ésta empieza a las 18:10, con la carpeta de
   GitHub). No era la de agosto cifrada: era **una nueva**.
3. La app de Claude y la de Codex son **apps empaquetadas (MSIX)**. Windows
   le da a cada una su propia copia de `AppData`
   (`%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Local\…`): lo
   que se lanza desde ellas —su terminal, lo que se abre desde ahí— escribe lo
   nuevo en esa copia y ve las dos carpetas **mezcladas**; lo que se abre desde
   una PowerShell normal sólo ve la de verdad. Un proceso lanzado fuera de
   Claude (con WMI) enseñó una carpeta `obrain` distinta de la que veía Claude.

Lo que pasó, entonces: en agosto la base y el **modelo de búsqueda por
significado** se crearon desde Claude, y vivían en su copia privada. El 23 el
usuario abrió la app desde una PowerShell normal: no vio la base de agosto y
creó otra, cifrada, en la carpeta de verdad. La sesión de Claude veía las dos
a la vez, con la de agosto tapando a la nueva. **No hubo cifrado a medias**, y
`reanudar` no tenía la culpa. Pero la trampa seguía viva: la app del usuario,
abierta desde una PowerShell, **no tenía el modelo**.

El usuario lo pidió así (2026-09-23): *«la app debe de tener una base fija
para cuando ya la instale para siempre en mi compu»*, y el modelo *«en un
lugar fijo que la app siempre lo pueda encontrar»*.

## Decisión

1. **Los datos de obrain viven en `%USERPROFILE%\.obrain`**
   (`%USERPROFILE%\.obrain`): la base, su llave y `models\`. Fuera de
   `AppData` Windows no hace copias por aplicación: se comprobó escribiendo
   desde Claude en el perfil y leyendo desde fuera. **Fuera de OneDrive**: una
   base viva no se sincroniza, y nada sale de la computadora.
   Fuera de Windows no cambia nada (`$XDG_DATA_HOME/obrain`). `OBRAIN_DB`
   sigue mandando si está puesta.
2. **Todo sale de un solo sitio**, `obrain_core::ubicacion`: la ruta de la base
   (`obrain_api::default_db_path`) y la de los modelos
   (`obrain_embed::download::default_dir`).
3. **La mudanza es sola.** La primera vez que un proceso pregunta dónde está la
   base, se mueve de `%LOCALAPPDATA%\obrain` lo que falte en la carpeta nueva:
   la base con su WAL, su memoria compartida y su llave (la llave no depende de
   la ruta: la protege DPAPI con una entropía fija), y `models\`.
   - La base primero, y ella la última de los suyos: si otra obrain la tiene
     abierta, Windows no deja mover su WAL, no se mueve nada, y **esta vez se
     usa donde está** (abrir la carpeta nueva crearía una base vacía al lado,
     que es el fallo de partida). Se reintenta la vez siguiente.
   - No pisa nada: si ya hay base en la nueva, la nueva manda y se avisa de
     que hay otra en la vieja.
   - Se puede repetir: una mudanza cortada a medias termina la siguiente.
   - Los modelos se buscan en la nueva y, si no están, en la vieja.
4. **La base no despierta al vigía** aunque tu perfil entero sea una
   biblioteca. El escáner ya poda las carpetas con punto de la raíz del perfil;
   el vigía, que recibe rutas sueltas, aplica ahora la misma regla
   (`ExclusionSet::cuelga_de_dotdir_del_perfil`). Sin esto, cada pasada
   escribiría en la base y se despertaría a sí misma.
5. **El modelo del usuario se movió a mano** (2026-09-23) de la copia privada
   de Claude a `%USERPROFILE%\.obrain\models`: desde una PowerShell normal,
   la mudanza automática no lo habría visto nunca.

## Alternativas consideradas

### A. Dejar la base en AppData y avisar

obrain detectaría que lo abrió otra app empaquetada (`GetCurrentPackageFullName`)
y lo diría en rojo. Más barato, pero la trampa sigue ahí si no se lee el aviso,
y el modelo seguiría perdido.

### B. Dejarla en AppData y negarse a abrirla desde otra app empaquetada

Lo más seguro para los datos, pero ninguna sesión de Claude podría revisar la
base del usuario, que es como se encontraron este fallo y el del pasaporte.

### C. Salir de la virtualización desde dentro (lanzar la app «fuera» del paquete)

Windows permite a un proceso empaquetado lanzar hijos fuera del paquete, pero
depende de cómo esté declarado el paquete de otro (Claude, Codex) y no
arregla la orden `obrain` lanzada desde sus terminales.

### D. `Documentos\obrain`

Visible y fácil de respaldar, pero en este equipo `Documentos` está dentro de
OneDrive: la base se subiría a la nube, y OneDrive sincronizando un SQLite
abierto lo corrompe.

## Consecuencias

**A favor**

- Una sola base y un solo modelo, se abra obrain desde donde se abra: la app
  desde una PowerShell, desde la terminal de Claude, la orden `obrain` que
  lanza una sesión de Claude o de Codex.
- La app del usuario vuelve a buscar por significado sin descargar nada.
- La mudanza no pierde nada ni pisa nada, y se probó fuera de Claude con el
  ejecutable real: la base, su llave y el modelo se movieron y la base se
  abrió cifrada desde la carpeta nueva.

**En contra**

- **La carpeta está en la raíz del perfil**, donde Windows no espera datos de
  aplicación. Empieza por punto para que se lea como «de un programa», como
  `.ssh` o `.cargo`, y el escáner la trata así.
- Lo que la interfaz guarda en su navegador (el tema, los apartados plegados)
  sigue en `AppData` y puede ser distinto según desde dónde se abra la app. No
  es conocimiento: se vuelve a elegir con un clic.
- Lo que quedó de agosto en la copia privada de Claude (`vida.obrain-wal`,
  `respaldo-cifrado-a-medias-20260923`…) no se toca: sólo lo ve Claude, y
  borrarlo es decisión del usuario.
