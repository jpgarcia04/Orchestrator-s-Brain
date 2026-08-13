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
  method: string;
  confidence: number;
  source: string;
  evidence: string | null;
  by_user: boolean;
}

export interface VistaEntidad {
  id: string;
  label: string;
  path: string | null;
  facets: string[];
  sensitivity: Sensibilidad;
  observed: Claim[];
  inferred: Claim[];
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
}

export interface Estado {
  db: string;
  semantic: boolean;
  entities: number;
  assertions: number;
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
export const vistazo = (query: string, limit = 12) =>
  invoke<Vistazo>("vistazo", { query, limit });

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

export const entidad = (id: string) => invoke<VistaEntidad>("entidad", { id });

export const estado = () => invoke<Estado>("estado");
