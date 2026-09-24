/**
 * El puente al núcleo. **Tipos y nada más.**
 *
 * Todo lo que hay aquí es el reflejo exacto de lo que `core/obrain-api`
 * serializa. No hay una sola decisión en este archivo: ni qué es relevante, ni
 * qué es un hecho, ni cómo se llama un nodo. Si algo de eso apareciera aquí,
 * habría dos definiciones —una en Rust y otra en TypeScript— y el día que
 * discreparan el mapa mentiría sin que nadie lo notara.
 *
 * Es la respuesta concreta a por qué M5 empezó por un crate y no por una
 * ventana.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** `(rama, puesto)` — la procedencia de I9 en cada resultado. */
export type Rama = [string, number];

export type Sensibilidad = "normal" | "sensitive" | "vault";

export interface Hit {
  entity: string;
  label: string;
  path: string;
  snippet: string;
  score: number;
  ranks: Rama[];
  sensitivity: Sensibilidad;
  /**
   * El paquete de la lista bajo el que va: está dentro de él (ADR-0030). La
   * lista ya viene en ese orden; la fila no se esconde.
   */
  inside: string | null;
}

/** Un grupo de resultados: los que comparten carpeta. */
export interface Grupo {
  hub: string | null;
  label: string;
  /** Identificadores de los resultados. No copias: no se duplica nada. */
  members: string[];
  /** El mejor puesto que alcanzó un miembro. Ordena los grupos. */
  best: number;
  /** Carpetas por encima, de dentro hacia fuera → qué región encender. */
  ancestors: string[];
}

export interface Answer {
  hits: Hit[];
  /** Los mismos resultados agrupados. Doce archivos son casi siempre tres cosas. */
  clusters: Grupo[];
  /** Qué ramas de §6 corrieron. Hoy `bm25` y, con el modelo cargado, `knn`. */
  branches: string[];
  /** `false` mientras el modelo no esté listo: la respuesta es sólo léxica. */
  semantic: boolean;
  elapsed_ms: number;
}

export interface Nodo {
  id: string;
  label: string;
  /** Saltos desde el foco → **luminosidad**. */
  distance: number;
  /** Aristas vivas en todo el grafo, no sólo las visibles → **tamaño**. */
  degree: number;
  facets: string[];
  sensitivity: Sensibilidad;
  /** Posición de partida en el disco unidad, estable para siempre. */
  seed: [number, number];
  /** Cuándo se tocó por última vez, epoch ms. `0` = no se sabe. → latido. */
  recency: number;
}

export interface Arista {
  from: string;
  to: string;
  predicate: string;
  /** `deterministic` | `user` → sólida · el resto → punteada. */
  method: string;
  /** → **opacidad**. */
  confidence: number;
  source: string;
}

export interface Lente {
  focus: string[];
  nodes: Nodo[];
  edges: Arista[];
  budget: number;
  /** Hay más grafo del que se enseña. */
  truncated: boolean;
  reached: number;
}

export interface Claim {
  assertion: string;
  predicate: string;
  label: string;
  value: string;
  value_entity: string | null;
  /** Si es un instante, en epoch ms: la fecha se escribe en tu zona horaria. */
  value_time: number | null;
  method: string;
  confidence: number;
  source: string;
  evidence: string | null;
  by_user: boolean;
  /**
   * El tramo del documento que lo explica, en bytes del texto entero: la
   * fecha de caducidad y la sensibilidad que salió del contenido.
   */
  span: [number, number] | null;
  /**
   * El tramo en el texto **del otro** documento: dónde está éste dentro de su
   * paquete (`part_of`, ADR-0030). Es a donde lleva «Está dentro de».
   */
  target_span: [number, number] | null;
}

/** Un documento dentro de otro (ADR-0030). */
export interface Parte {
  id: string;
  label: string;
  /** Dónde está en el texto del paquete, en bytes. */
  span: [number, number] | null;
  /** Párrafos suyos que están en el paquete de un tirón, de `of`. */
  paragraphs: number;
  of: number;
  sensitivity: Sensibilidad;
  /** Está en la papelera: se sigue diciendo que estaba dentro. */
  trashed: boolean;
}

/**
 * El documento de dentro que hace sensible a éste (ADR-0030): el PDF de la
 * visa es sensible porque contiene la carátula del pasaporte.
 */
export interface PorDentro {
  part: string;
  label: string;
  level: "sensitive" | "vault";
  /**
   * Por qué lo es él: una carpeta que marcaste, lo dijiste tú, su texto, su
   * nombre, el nombre de su carpeta, o no se sabe más.
   */
  because: "folder" | "user" | "content" | "name" | "folder_name" | "classifier";
  folder: Proteccion | null;
}

/**
 * La carpeta marcada que protege algo (ADR-0020): la respuesta a «¿por qué es
 * sensible esto?» cuando no hay una aserción sobre el propio documento.
 */
export interface Proteccion {
  /** La carpeta marcada. */
  folder: string;
  label: string;
  level: "sensitive" | "vault";
  /** La marca es de esta misma carpeta, no de una que la contiene. */
  own: boolean;
}

/** Una carpeta que marcaste como sensible, para la vista «Sensibles». */
export interface CarpetaMarcada {
  id: string;
  label: string;
  path: string;
  level: "sensitive" | "vault";
  /** Documentos que tiene dentro ahora. Cero suele ser una carpeta renombrada. */
  documents: number;
  /** Desde cuándo, epoch ms. */
  since: number;
}

/** Lo que pasó al marcar o desmarcar una carpeta. */
export interface Marcado {
  /** La marca que queda puesta, o `null` si se quitó. */
  level: "sensitive" | "vault" | null;
  /** Lo que hay dentro —ella incluida— recalculado. */
  under: number;
  /** De eso, lo que ha cambiado de nivel. */
  changed: number;
}

export interface VistaEntidad {
  id: string;
  label: string;
  path: string | null;
  facets: string[];
  sensitivity: Sensibilidad;
  /** La carpeta marcada que decide su nivel, si es una carpeta quien lo decide. */
  protection: Proteccion | null;
  /** El documento que lleva dentro y que decide su nivel, si es él (ADR-0030). */
  by_part: PorDentro | null;
  /** Lo que lleva dentro, en el orden de su texto (ADR-0030). */
  parts: Parte[];
  observed: Claim[];
  inferred: Claim[];
  /** Está en la papelera: ya no está en tu bóveda (ADR-0025). */
  trashed: EnPapelera | null;
  /** Las colecciones donde lo metiste (ADR-0031). */
  collections: Pertenece[];
}

/** Una colección tuya (ADR-0031), para la barra lateral y su vista. */
export interface Coleccion {
  id: string;
  name: string;
  /** Documentos que tiene en tu bóveda (sin contar la papelera). */
  documents: number;
  /** De los suyos, cuántos están en la papelera. */
  in_trash: number;
  /** Desde cuándo existe, epoch ms. */
  since: number;
}

/** Una colección a la que pertenece un documento. */
export interface Pertenece {
  id: string;
  name: string;
}

/** Un documento de una colección abierta: una {@link Fila} y su carpeta. */
export interface Miembro extends Fila {
  /** Su carpeta de verdad: en una colección cada uno viene de un sitio. */
  folder: string;
  folder_label: string;
  /** El paquete de la colección bajo el que va (ADR-0030). */
  inside: string | null;
  /** Las carpetas por encima, de dentro hacia fuera: qué estrella encender. */
  ancestors: string[];
}

/** Una colección abierta. */
export interface Abierta {
  id: string;
  name: string;
  since: number;
  rows: Miembro[];
  /** En cuántas carpetas distintas están sus documentos. */
  folders: number;
  /** Los suyos que están en la papelera. */
  in_trash: EnPapelera[];
}

/** Un documento de la papelera: lo que ya no está en tu bóveda (ADR-0025). */
export interface EnPapelera {
  id: string;
  /** Su nombre, el de la última ruta donde estuvo. */
  label: string;
  /** La última ruta donde estuvo. */
  path: string;
  /** La carpeta donde estaba. */
  folder: string | null;
  sensitivity: Sensibilidad;
  /** Cuándo se enteró obrain de que ya no estaba (epoch ms). */
  since: number;
  /** Días hasta que se olvide; `0` es hoy. */
  days_left: number;
}

/** Una región del cielo. Una «estrella». */
export interface Region {
  id: string;
  label: string;
  path: string;
  /** Archivos que contiene, contando subcarpetas → tamaño. */
  weight: number;
  depth: number;
  /** Sólo lo que cuelga directamente: el resto se abrió en trozos. */
  remainder: boolean;
  seed: [number, number];
  sensitivity: Sensibilidad;
  /** Lo más reciente que hay dentro, epoch ms → **temperatura**. */
  recency: number;
}

/** Dos regiones que una sesión de trabajo cruzó. */
export interface Puente {
  from: string;
  to: string;
  weight: number;
}

export interface Cielo {
  regions: Region[];
  /** Los más fuertes, hasta el presupuesto. */
  bridges: Puente[];
  /** Cuántos había antes de recortar. */
  bridges_found: number;
  files: number;
  covered: number;
  target: number;
  /** Rango temporal del corpus, para normalizar la recencia. */
  oldest: number;
  newest: number;
}

/** Un escalón de las migas de pan. */
export interface Miga {
  id: string;
  label: string;
}

/**
 * Una fila del explorador.
 *
 * `availability` es la guarda: `"local"` es lo único que se puede leer. `null`
 * **no es lo mismo que local** — el núcleo devuelve `None` ante un valor que no
 * reconoce, a propósito, porque ante la duda no se toca el archivo.
 */
export interface Fila {
  id: string;
  label: string;
  path: string;
  kind: "folder" | "file";
  /** Archivos dentro, contando subcarpetas. `0` en un archivo. */
  weight: number;
  sensitivity: Sensibilidad;
  availability: string | null;
  recency: number;
  facets: string[];
}

/**
 * Un documento que caduca pronto o ya caducó: una fila de la tarjeta «Caduca
 * pronto» (ADR-0023). Es una {@link Fila} más lo que dice de su fecha.
 */
export interface Vencimiento extends Fila {
  /** El nombre de la carpeta donde vive: el contexto de un `IMG_2291.jpg`. */
  folder: string | null;
  /** `AAAA-MM-DD`. `null` si es sensible y el PIN lo tiene bloqueado. */
  date: string | null;
  /** Días de hoy (UTC, según el núcleo) a la fecha; negativo si caducó. */
  days: number | null;
  /** La fecha la dijo el usuario (I5). */
  by_user: boolean;
  /** Sensible y bloqueado: se sabe que está aquí, no cuándo caduca. */
  locked: boolean;
}

/** Lo que hay dentro de un contenedor, con la ruta hasta él. */
export interface Listado {
  focus: string | null;
  label: string;
  path: string;
  /** De fuera hacia dentro. */
  trail: Miga[];
  rows: Fila[];
  folders: number;
  files: number;
  /** La marca de sensibilidad que la afecta: la suya o la de una carpeta de fuera. */
  protection: Proteccion | null;
}

/**
 * De un desplazamiento en **bytes UTF-8** —lo que cuenta Rust— al índice de
 * JavaScript, que cuenta **unidades UTF-16**.
 *
 * No es un detalle: en un texto en español cada «á» o «ñ» son dos bytes y una
 * sola unidad, así que usar el número tal cual corre el resaltado a la derecha
 * tantas posiciones como acentos haya antes. Con documentos personales en
 * español, eso es siempre.
 */
export function desdeBytes(texto: string, byte: number): number {
  let b = 0;
  let i = 0;
  while (i < texto.length && b < byte) {
    const c = texto.codePointAt(i) ?? 0;
    b += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    i += c > 0xffff ? 2 : 1;
  }
  return i;
}

/**
 * Qué se ve del archivo.
 *
 * `readable` es la guarda de M1 **ya resuelta por el núcleo**. No la vuelvas a
 * deducir de `availability` aquí: son dos definiciones de lo mismo, y el día
 * que discrepen se hidrata un puntero de OneDrive sin que nadie lo note.
 */
export interface Vista {
  id: string;
  path: string | null;
  availability: string | null;
  readable: boolean;
  sensitivity: Sensibilidad;
  kind: "text" | "image" | "none";
  text: string | null;
  /** Byte del documento donde empieza `text`. */
  offset: number;
  /** Caracteres del documento **entero**, no del tramo que llegó. */
  chars: number;
  truncated: boolean;
  /**
   * `[inicio, fin)` dentro de `text`: el tramo que explica la consulta.
   * **En bytes UTF-8**, como los cuenta Rust: para cortar el texto en
   * JavaScript hay que pasarlos por [`desdeBytes`](#desdeBytes).
   */
  highlight: [number, number] | null;
  /** Ya como `data:` URI. La genera el núcleo, no la ventana. */
  image: string | null;
  width: number;
  height: number;
  /** Por qué no hay nada que enseñar. §4.6: se dice, no se deja en blanco. */
  reason: string | null;
  /**
   * Es sensible, hay PIN y no está desbloqueado: el contenido **no ha salido
   * del núcleo** (ADR-0021). No hay nada que destapar hasta escribir el PIN.
   */
  locked: boolean;
}

export interface Estado {
  db: string;
  /** Windows no tiene la llave de la base: hace falta la clave de recuperación. */
  cerrada: boolean;
  semantic: boolean;
  entities: number;
  assertions: number;
  /** Las carpetas que elegiste indexar. Vacío = bóveda sin empezar. */
  libraries: string[];
  /** Hay un reindexado en marcha. */
  indexing: boolean;
  /** El OCR de este equipo (`windows.media.ocr/es-ES`), o `null` si no hay. */
  ocr: string | null;
}

/** Cómo está la seguridad de la base (ADR-0021). */
export interface Seguridad {
  /** La base está cifrada en el disco. */
  cifrada: boolean;
  /** Quién guarda la llave: `dpapi` (Windows) o `sin-proteger` (banco de pruebas). */
  proteccion: string | null;
  /** Hay una clave de recuperación que todavía no has confirmado haber guardado. */
  recuperacion_pendiente: boolean;
  pin: boolean;
  /** Con PIN: lo sensible se puede ver ahora mismo. */
  desbloqueada: boolean;
  /** Tras varios fallos: cuánto falta para volver a probar. */
  espera_ms: number;
}

/** Lo que hizo un reindexado. Ver `obrain_api::IndexReport`. */
export interface Informe {
  libraries: string[];
  missing: string[];
  files_seen: number;
  files_new: number;
  edges_new: number;
  documents_read: number;
  chunks_new: number;
  /** De los documentos leídos, los que se leyeron con OCR. */
  ocr: number;
  sensitive: number;
  vault: number;
  /** De los sensibles, los que lo son por estar en una carpeta marcada. */
  inherited: number;
  /** Documentos con fecha de caducidad leída de su texto. */
  expiring: number;
  /** Pasaron a la papelera, volvieron de ella y se olvidaron (ADR-0025). */
  trashed: number;
  returned: number;
  forgotten: number;
  vectors_new: number;
  semantic: boolean;
  elapsed_ms: number;
}

export interface Indexado {
  report: Informe | null;
  error: string | null;
  /** La lanzó el vigía al ver un cambio, no tú: no se avisa (ADR-0024). */
  live: boolean;
}

/** Una consulta y lo que enciende, en una sola llamada. */
export interface Vistazo {
  answer: Answer;
  lens: Lente;
}

/**
 * Lo que llama la barra en cada pulsación.
 *
 * Una sola llamada y no dos: pedir la lista y el sustrato por separado
 * ejecutaría la búsqueda dos veces por tecla.
 */
export const vistazo = (query: string, limit = 12, coleccion: string | null = null) =>
  invoke<Vistazo>("vistazo", { query, limit, coleccion });

// ── tus colecciones (ADR-0031): identificadores, nunca rutas; ningún archivo se toca ──

export const colecciones = () => invoke<Coleccion[]>("colecciones");

export const coleccion = (id: string) => invoke<Abierta>("coleccion", { id });

/** Crea una colección vacía; devuelve su identificador. */
export const crearColeccion = (nombre: string) => invoke<string>("crear_coleccion", { nombre });

export const renombrarColeccion = (id: string, nombre: string) =>
  invoke<void>("renombrar_coleccion", { id, nombre });

/** Borra la colección; tus documentos no se tocan. Devuelve cuántos tenía. */
export const borrarColeccion = (id: string) => invoke<number>("borrar_coleccion", { id });

/** Mete documentos; devuelve cuántos entraron (los que ya estaban no cuentan). */
export const meterEnColeccion = (id: string, docs: string[]) =>
  invoke<number>("meter_en_coleccion", { id, docs });

/** Saca documentos; siguen en sus carpetas. Devuelve cuántos salieron. */
export const sacarDeColeccion = (id: string, docs: string[]) =>
  invoke<number>("sacar_de_coleccion", { id, docs });

export const lente = (focus: string[], radius = 2, budget = 300) =>
  invoke<Lente>("lente", { focus, radius, budget });

/** Lo que se ve al abrir: el grafo entero a poca escala. */
export const cielo = (target = 30) => invoke<Cielo>("cielo", { target });

/**
 * Qué hay dentro de esto. El explorador de ADR-0014.
 *
 * Sin foco entra por las raíces. **Es la misma lectura del árbol que `cielo`**,
 * un escalón más abajo: por eso las dos vistas no pueden discrepar sobre a qué
 * pertenece un archivo.
 */
export const explorar = (foco: string | null = null) =>
  invoke<Listado>("explorar", { foco });

/** Lo último que tocaste: el «Recientes» del inicio. */
export const recientes = (limit = 8) => invoke<Fila[]>("recientes", { limit });

/** Lo que la puerta de egreso retiene: la vista «Sensibles». */
export const sensibles = () => invoke<Fila[]>("sensibles");

/** Lo que caduca en 90 días o ya caducó: la tarjeta del inicio. */
export const vencimientos = () => invoke<Vencimiento[]>("vencimientos");

/**
 * «cambiar» en la vista previa: cuándo caduca un documento (`AAAA-MM-DD`), o
 * `null` para «no caduca». Una aserción tuya, que gana para siempre (I5).
 */
export const corregirVencimiento = (id: string, fecha: string | null) =>
  invoke<string>("corregir_vencimiento", { id, fecha });

/** Lo que ya no está en tu bóveda y aún no se ha olvidado (ADR-0025). */
export const papelera = () => invoke<EnPapelera[]>("papelera");

/**
 * «Olvidar ya» (con `id`) o «Vaciar papelera» (con `null`): borra el texto y
 * los vectores que obrain guardaba. No toca tus archivos ni el log.
 */
export const olvidar = (id: string | null) => invoke<number>("olvidar", { id });

/** Las carpetas que marcaste como sensibles. */
export const carpetasMarcadas = () => invoke<CarpetaMarcada[]>("carpetas_marcadas");

/**
 * Marca una carpeta como sensible —todo lo que tenga dentro lo hereda, también
 * lo que llegue después— o, con `null`, le quita la marca. Es una aserción
 * tuya (I5): ningún reescaneo la pisa.
 */
export const marcarCarpeta = (carpeta: string, nivel: "sensitive" | "vault" | null) =>
  invoke<Marcado>("marcar_carpeta", { carpeta, nivel });

export const entidad = (id: string) => invoke<VistaEntidad>("entidad", { id });

/**
 * Qué se ve del archivo.
 *
 * `consulta` sólo decide **dónde se abre** el texto —el tramo que explica lo
 * que se buscó, igual que la prueba de cada fila de resultados— y nunca cuánto
 * se puede leer.
 */
/**
 * Lo que se ve de un documento. `consulta` y `tramo` sólo deciden por dónde se
 * abre; el tramo —dónde está algo que lleva dentro (ADR-0030)— manda.
 */
export const previsualizar = (id: string, consulta?: string, tramo?: [number, number] | null) =>
  invoke<Vista>("previsualizar", { id, consulta: consulta ?? null, tramo: tramo ?? null });

/**
 * Abre el archivo con el programa del sistema.
 *
 * **Va por identificador, no por ruta**, y no es un detalle de transporte: un
 * comando que aceptara rutas sería un «abre esto» genérico expuesto a esta
 * ventana, y la aplicación pasaría de poder tocar lo indexado a poder tocar
 * cualquier cosa del disco.
 */
export const abrir = (id: string) => invoke<void>("abrir", { id });

/** Lo revela en el explorador de Windows. Nunca lo ejecuta. */
export const revelar = (id: string) => invoke<void>("revelar", { id });

/** La corrección de I5. Gana para siempre y ningún reescaneo la pisa. */
export const corregir = (subject: string, predicate: string, object: string) =>
  invoke<string>("corregir", { subject, predicate, object });

export const estado = () => invoke<Estado>("estado");

// ── seguridad (ADR-0021) ──

export const seguridad = () => invoke<Seguridad>("seguridad");

/** La clave de recuperación, para enseñarla. Con PIN, sólo desbloqueado. */
export const claveRecuperacion = () => invoke<string | null>("clave_recuperacion");

/** «Ya la guardé»: la app deja de enseñarla al abrir. */
export const recuperacionGuardada = () => invoke<void>("recuperacion_guardada");

/** Abre la base con la clave de recuperación cuando Windows no tiene su llave. */
export const recuperar = (codigo: string) => invoke<void>("recuperar", { codigo });

export const pinDesbloquear = (pin: string) => invoke<void>("pin_desbloquear", { pin });
export const pinBloquear = () => invoke<void>("pin_bloquear");
export const pinPoner = (nuevo: string, actual: string | null) => invoke<void>("pin_poner", { nuevo, actual });
export const pinQuitar = (actual: string) => invoke<void>("pin_quitar", { actual });

/** Olvidé el PIN: con la clave de recuperación se pone otro (o ninguno, con `null`). */
export const pinRestablecer = (codigo: string, nuevo: string | null) =>
  invoke<void>("pin_restablecer", { codigo, nuevo });

/**
 * Vuelve a leer tus bibliotecas. Vuelve al instante —`false` si ya había un
 * reindexado en marcha— y el resultado llega por [`alIndexar`].
 */
export const actualizar = () => invoke<boolean>("actualizar");

/**
 * Añade una carpeta a tu bóveda. **El selector de carpetas lo abre Rust**, no
 * esta ventana: así ninguna ruta viaja desde aquí hasta el núcleo y la
 * ventana no puede pedir que se indexe cualquier sitio del disco. Devuelve
 * `false` si se canceló o si ya había un reindexado en marcha; el resultado
 * llega por [`alIndexar`].
 */
export const anadirBiblioteca = () => invoke<boolean>("anadir_biblioteca");

/**
 * Avisa cada vez que termina un reindexado: al arrancar la aplicación y tras
 * cada «actualizar». Si el canal de eventos no existe (la interfaz abierta en
 * un navegador de pruebas), no avisa y no rompe nada.
 */
export async function alIndexar(f: (i: Indexado) => void): Promise<void> {
  try {
    await listen<Indexado>("indexado", (e) => f(e.payload));
  } catch {
    // sin eventos: la interfaz funciona igual, sólo que no se refresca sola
  }
}

/**
 * Avisa cuando **empieza** una pasada que no pediste: la del vigía, al ver
 * que algo cambió en tus bibliotecas (ADR-0024). Las que pides tú ya las
 * sabe la ventana.
 */
export async function alEmpezarIndexado(f: () => void): Promise<void> {
  try {
    await listen("indexando", () => f());
  } catch {
    // sin eventos, igual que arriba
  }
}
