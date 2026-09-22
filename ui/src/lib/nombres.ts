/**
 * Cómo se le enseña a una persona lo que el núcleo llama por su nombre de
 * archivo.
 *
 * Nada de esto cambia el disco ni el núcleo: `01 Identidad` sigue llamándose
 * así en tu carpeta, porque el número es lo que la ordena en el explorador de
 * Windows. Aquí sólo se decide **cómo se lee**.
 */

/**
 * `01 Identidad` → `Identidad`.
 *
 * El número delante es para ordenar en el disco, no parte del nombre. Sólo se
 * quita si es corto (hasta tres cifras) y va seguido de un separador, así que
 * `2026-09-01 Póliza` no se toca.
 */
export function nombreBonito(nombre: string): string {
  const limpio = nombre.replace(/^\d{1,3}[\s._-]+(?=\S)/, "");
  return limpio || nombre;
}

/** Lo que hay antes de la última barra de una ruta: su carpeta, ya bonita. */
export function carpetaDe(ruta: string): string {
  const partes = ruta.split(/[\\/]/).filter(Boolean);
  return nombreBonito(partes[partes.length - 2] ?? "");
}

/** La hoja de una ruta. */
export function hojaDe(ruta: string): string {
  const partes = ruta.split(/[\\/]/).filter(Boolean);
  return partes[partes.length - 1] ?? ruta;
}

const MESES = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"];

/** Un archivo leído como documento: su título y, si lo lleva, su fecha. */
export interface Titulo {
  titulo: string;
  /** La fecha con la que empieza el nombre (`2026-09-01 …`), ya escrita. */
  fecha: string | null;
}

/**
 * `2026-09-01 Póliza seguro médico.pdf` → «Póliza seguro médico», con fecha
 * «1 sept 2026».
 *
 * Es la convención que propone `docs/ESTADO.md` para nombrar la bóveda: la
 * fecha delante para que el disco ordene solo, y el resto para leerlo. Los
 * guiones bajos se leen como espacios.
 */
export function tituloDe(nombreArchivo: string): Titulo {
  let base = nombreArchivo.replace(/\.[A-Za-z0-9]{1,5}$/, "");
  let fecha: string | null = null;
  const m = /^(\d{4})-(\d{2})-(\d{2})[\s_-]+(.+)$/.exec(base);
  if (m) {
    const mes = MESES[Number(m[2]) - 1];
    if (mes) {
      fecha = `${Number(m[3])} ${mes} ${m[1]}`;
      base = m[4] ?? base;
    }
  }
  const titulo = base.replace(/_+/g, " ").replace(/\s+/g, " ").trim();
  return { titulo: titulo || nombreArchivo, fecha };
}

/** Los tipos que se distinguen a la vista. */
export type Tipo = "PDF" | "IMG" | "DOC" | "XLS" | "PPT" | "TXT" | "WEB" | "COD" | "ZIP" | "VID" | "AUD" | "···";

const POR_EXTENSION: Record<string, Tipo> = {
  pdf: "PDF",
  jpg: "IMG", jpeg: "IMG", png: "IMG", gif: "IMG", webp: "IMG", bmp: "IMG", heic: "IMG",
  doc: "DOC", docx: "DOC", odt: "DOC", rtf: "DOC",
  xls: "XLS", xlsx: "XLS", ods: "XLS", csv: "XLS",
  ppt: "PPT", pptx: "PPT", odp: "PPT",
  txt: "TXT", md: "TXT", log: "TXT",
  html: "WEB", htm: "WEB",
  java: "COD", rs: "COD", ts: "COD", js: "COD", py: "COD", sql: "COD", json: "COD", xml: "COD",
  zip: "ZIP", rar: "ZIP", "7z": "ZIP",
  mp4: "VID", mov: "VID", mkv: "VID",
  mp3: "AUD", m4a: "AUD", wav: "AUD",
};

export function tipoDe(nombreArchivo: string): Tipo {
  const ext = /\.([A-Za-z0-9]{1,5})$/.exec(nombreArchivo)?.[1]?.toLowerCase() ?? "";
  return POR_EXTENSION[ext] ?? "···";
}

const LARGO: Record<Tipo, string> = {
  PDF: "PDF", IMG: "Imagen", DOC: "Word", XLS: "Hoja de cálculo", PPT: "Presentación",
  TXT: "Texto", WEB: "Página web", COD: "Código", ZIP: "Comprimido", VID: "Vídeo", AUD: "Audio", "···": "Archivo",
};

export const tipoLargo = (t: Tipo) => LARGO[t];

/** La variable de color de cada tipo, definida en `estilos/tema.css`. */
export function colorDeTipo(t: Tipo): string {
  switch (t) {
    case "PDF": return "var(--tipo-pdf)";
    case "IMG": return "var(--tipo-img)";
    case "DOC": return "var(--tipo-doc)";
    case "XLS": return "var(--tipo-xls)";
    case "PPT": return "var(--tipo-ppt)";
    case "TXT": case "COD": case "WEB": return "var(--tipo-txt)";
    default: return "var(--tenue)";
  }
}

/** `epoch ms` → «2 sept 2026». Vacío si no se sabe. */
export function fecha(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  return `${d.getDate()} ${MESES[d.getMonth()]} ${d.getFullYear()}`;
}

/** `epoch ms` → «hace 3 días». Para lo reciente; lo viejo va con fecha. */
export function haceCuanto(ms: number, ahora = Date.now()): string {
  if (!ms) return "";
  const s = Math.round((ahora - ms) / 1000);
  const rtf = new Intl.RelativeTimeFormat("es", { numeric: "auto" });
  if (s < 60) return "ahora mismo";
  if (s < 3600) return rtf.format(-Math.round(s / 60), "minute");
  if (s < 86400) return rtf.format(-Math.round(s / 3600), "hour");
  if (s < 86400 * 30) return rtf.format(-Math.round(s / 86400), "day");
  return fecha(ms);
}

/** Saludo según la hora. */
export function saludo(d = new Date()): string {
  const h = d.getHours();
  if (h < 6) return "Buenas noches";
  if (h < 13) return "Buenos días";
  if (h < 20) return "Buenas tardes";
  return "Buenas noches";
}
