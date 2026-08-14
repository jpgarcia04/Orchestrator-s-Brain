/**
 * El flujo.
 *
 * Abres y **ves tu mapa**: treinta regiones con nombre, las grandes brillando
 * más. Si quieres algo concreto, escribes — y el mapa se enciende por donde cae
 * la respuesta, como buscar una calle en un mapa que ya estabas mirando.
 *
 * Es la corrección de [ADR-0004], que puso la barra delante y el mapa detrás.
 * Sobrevive de aquella decisión la parte que era buena —*el mapa reacciona
 * mientras escribes*— y cae la que no: que el mapa fuera un fondo.
 *
 * ## Lo que deliberadamente NO hace
 *
 * No hay tecla global, ni modo deriva, ni expansión, ni las seis animaciones de
 * §3. Salieron de M5b por escrito cuando el hito se encogió. Lo que hay aquí es
 * el bucle completo —ver, buscar, acercarse, **abrir**, volver— porque un bucle
 * incompleto no se puede juzgar.
 *
 * [ADR-0004]: ../../docs/adr/0004-punto-de-entrada-barra-no-galaxia.md
 */

import { Explorador } from "./explorador";
import { Mapa } from "./mapa";
import { Panel } from "./panel";
import { cielo, estado, lente, vistazo } from "./nucleo";
import type { Cielo, Grupo, Hit } from "./nucleo";

const $consulta = document.querySelector<HTMLInputElement>("#consulta")!;
const $estado = document.querySelector<HTMLDivElement>("#estado")!;
const $resultados = document.querySelector<HTMLOListElement>("#resultados")!;
const $lienzo = document.querySelector<HTMLCanvasElement>("#sustrato")!;
const $barra = document.querySelector<HTMLElement>("#barra")!;
const $conmutador = document.querySelector<HTMLButtonElement>("#conmutador")!;

const $detalle = document.querySelector<HTMLElement>("#detalle")!;

const mapa = new Mapa($lienzo);
const explorador = new Explorador(document.querySelector<HTMLElement>("#explorador")!);
const panel = new Panel(document.querySelector<HTMLElement>("#panel")!);

/**
 * Lo último que se escribió. El panel lo usa para abrir el documento **donde
 * está la respuesta** en vez de por el principio (ADR-0016).
 */
let ultimaConsulta = "";

/**
 * Espera antes de previsualizar.
 *
 * La flecha del explorador cambia de fila más rápido de lo que se lee, y cada
 * fila puede costar una miniatura de 25 ms y 85 KB. Sin esto, recorrer una
 * carpeta de capturas genera una imagen por pulsación para enseñar sólo la
 * última. El número es más alto que el de la barra porque previsualizar es más
 * caro que buscar, y porque nadie lee lo que pasa de largo.
 */
const ESPERA_PANEL = 140;
let pendientePanel: number | undefined;

/** Enseña una entidad en el panel, sin ahogarse si las peticiones se agolpan. */
function ver(id: string, ya = false, consulta = ultimaConsulta): void {
  window.clearTimeout(pendientePanel);
  const mostrar = () => void panel.mostrar(id, consulta);
  if (ya) mostrar();
  else pendientePanel = window.setTimeout(mostrar, ESPERA_PANEL);
}

/**
 * Lo último que se enfocó. **Es la mitad de la continuidad del ADR-0014**: el
 * conmutador cambia la lente, no el destino, así que el explorador tiene que
 * poder abrir donde estaba mirando la galaxia.
 */
let foco: string | null = null;

/**
 * Espera antes de consultar. 90 ms y no 300: la consulta híbrida completa se
 * midió en 43 ms (informe de M4), así que puede quedarse por debajo de lo que
 * se percibe como retardo. Un debounce largo es lo que hace que una barra local
 * se sienta como una barra de internet.
 */
const ESPERA = 90;

let pendiente: number | undefined;
/** Descarta respuestas que llegan tarde: escribir rápido las desordena. */
let generacion = 0;
let miCielo: Cielo | null = null;
let grupos: Grupo[] = [];
let porId = new Map<string, Hit>();
let seleccion = 0;
/** Filas navegables, aplanadas: cabeceras de grupo y resultados. */
let filas: Array<{ grupo: number; hit?: string }> = [];

async function arrancar(): Promise<void> {
  await mapa.init();
  $consulta.focus();
  try {
    miCielo = await cielo(30);
    mapa.cielo(miCielo);
    const e = await estado();
    decir(
      `${miCielo.regions.length} regiones · ${miCielo.covered.toLocaleString("es")} de ` +
        `${miCielo.files.toLocaleString("es")} archivos${e.semantic ? "" : " · cargando el modelo…"}`,
    );
  } catch (err) {
    decir(`no se pudo abrir la base — ${err}`);
  }
}

function decir(texto: string): void {
  $estado.textContent = texto;
}

/** ¿En qué región del cielo cae este grupo? La primera de su ascendencia. */
function regionDe(g: Grupo): string | null {
  if (!miCielo) return null;
  const regiones = new Set(miCielo.regions.map((r) => r.id));
  if (g.hub && regiones.has(g.hub)) return g.hub;
  for (const a of g.ancestors) if (regiones.has(a)) return a;
  return null;
}

function pintarGrupos(): void {
  filas = [];
  const nodos: Node[] = [];

  grupos.forEach((g, gi) => {
    filas.push({ grupo: gi });
    const cabecera = document.createElement("li");
    cabecera.className = "grupo";
    cabecera.dataset["fila"] = String(filas.length - 1);
    const nombre = document.createElement("span");
    nombre.className = "grupo-nombre";
    nombre.textContent = g.label;
    const cuantos = document.createElement("span");
    cuantos.className = "grupo-cuenta";
    cuantos.textContent = g.members.length === 1 ? "1 archivo" : `${g.members.length} archivos`;
    cabecera.append(nombre, cuantos);
    nodos.push(cabecera);

    for (const id of g.members) {
      const h = porId.get(id);
      if (!h) continue;
      filas.push({ grupo: gi, hit: id });
      const li = document.createElement("li");
      li.className = "hit";
      li.dataset["fila"] = String(filas.length - 1);

      const linea = document.createElement("div");
      linea.className = "nombre";
      const etiqueta = document.createElement("strong");
      etiqueta.textContent = h.label;
      linea.append(etiqueta);
      if (h.sensitivity !== "normal") {
        const marca = document.createElement("span");
        marca.className = "boveda";
        marca.textContent = h.sensitivity === "vault" ? "BÓVEDA" : "SENSIBLE";
        linea.append(marca);
      }
      // I9 en cada fila: por qué está aquí, y no sólo que está.
      const ramas = document.createElement("span");
      ramas.className = "ramas";
      ramas.textContent = h.ranks.map(([r, p]) => `${r}#${p}`).join(" ");
      linea.append(ramas);
      li.append(linea);

      if (h.snippet) {
        const frag = document.createElement("div");
        frag.className = "fragmento";
        frag.textContent = h.snippet;
        li.append(frag);
      }
      nodos.push(li);
    }
  });

  $resultados.replaceChildren(...nodos);
  seleccion = 0;
  marcar();
}

function marcar(): void {
  for (const el of $resultados.children) {
    el.setAttribute("aria-selected", el.getAttribute("data-fila") === String(seleccion) ? "true" : "false");
  }
  $resultados
    .querySelector(`[data-fila="${seleccion}"]`)
    ?.scrollIntoView({ block: "nearest" });
}

function mover(delta: number): void {
  if (filas.length === 0) return;
  seleccion = (seleccion + delta + filas.length) % filas.length;
  marcar();
}

/** Enter: acercarse. Sobre un grupo abre su carpeta; sobre un archivo, el archivo. */
async function enfocar(): Promise<void> {
  const fila = filas[seleccion];
  if (!fila) return;
  const g = grupos[fila.grupo];
  if (!g) return;
  const donde = fila.hit ?? g.hub;
  if (!donde) return;
  foco = donde;
  // La mitad «y lo ves» del hito. Sin esto, acercarse enseña un vecindario de
  // puntos sobre un archivo que sigue sin poder abrirse.
  ver(donde, true);
  const l = await lente([donde], 2, 300);
  mapa.mostrar(l);
  const nombre = fila.hit ? porId.get(fila.hit)?.label : g.label;
  const base = `${nombre} · ${l.nodes.length} nodos, ${l.edges.length} aristas`;
  // Un mapa que esconde que enseña una parte miente sobre el tamaño de tu vida.
  decir(l.truncated ? `${base} · de ${l.reached} alcanzados · Esc para volver` : `${base} · Esc para volver`);
}

function alCielo(): void {
  if (miCielo) mapa.cielo(miCielo);
}

async function consultar(texto: string): Promise<void> {
  const mia = ++generacion;
  if (texto.trim() === "") {
    ultimaConsulta = "";
    grupos = [];
    porId.clear();
    $resultados.replaceChildren();
    filas = [];
    alCielo();
    if (miCielo) {
      decir(`${miCielo.regions.length} regiones · ${miCielo.files.toLocaleString("es")} archivos`);
    }
    return;
  }

  try {
    const v = await vistazo(texto, 40);
    if (mia !== generacion) return; // llegó tarde

    ultimaConsulta = texto;
    grupos = v.answer.clusters;
    porId = new Map(v.answer.hits.map((h) => [h.entity, h]));
    pintarGrupos();

    // El mapa no cambia de escala al escribir: se **enciende**. Cambiarlo
    // obligaría a reaprender dónde está todo en cada tecla.
    alCielo();
    const encendidas = grupos.map(regionDe).filter((r): r is string => r !== null);
    mapa.encender(encendidas);

    const ramas = v.answer.branches.join(" + ");
    const aviso = v.answer.semantic ? "" : " · sólo léxico, el modelo aún no está";
    const cosas = grupos.length === 1 ? "1 sitio" : `${grupos.length} sitios`;
    decir(
      `${v.answer.hits.length} archivos en ${cosas} · ${v.answer.elapsed_ms.toFixed(0)} ms · ${ramas}${aviso}`,
    );
  } catch (err) {
    if (mia !== generacion) return;
    decir(`${err}`);
  }
}

$consulta.addEventListener("input", () => {
  window.clearTimeout(pendiente);
  const texto = $consulta.value;
  pendiente = window.setTimeout(() => void consultar(texto), ESPERA);
});

$consulta.addEventListener("keydown", (e) => {
  switch (e.key) {
    case "ArrowDown":
      e.preventDefault();
      mover(1);
      break;
    case "ArrowUp":
      e.preventDefault();
      mover(-1);
      break;
    case "Enter":
      e.preventDefault();
      void enfocar();
      break;
    case "Escape":
      e.preventDefault();
      if ($consulta.value === "") alCielo();
      else {
        $consulta.value = "";
        void consultar("");
      }
      break;
  }
});

$resultados.addEventListener("click", (e) => {
  const li = (e.target as HTMLElement).closest("li");
  const fila = li?.dataset["fila"];
  if (fila === undefined) return;
  seleccion = Number(fila);
  marcar();
  void enfocar();
});

// Clic en el mapa. **El nodo manda sobre la región**: si hay una lente abierta,
// lo que está debajo del cursor es un nodo concreto, y tratarlo como región
// devolvería a quien acaba de llegar al sitio de donde venía.
//
// §4.4 en su forma literal: seleccionar un nodo abre el panel lateral y el mapa
// **sigue vivo detrás**. No es un modal, y por eso no hace falta la excepción
// que ADR-0014 tuvo que escribir para el explorador.
$lienzo.addEventListener("click", async (e) => {
  const n = mapa.nodoEn(e.clientX, e.clientY);
  if (n) {
    foco = n.id;
    ver(n.id, true);
    decir(`${n.label} · ${n.degree} relaciones · Esc para volver`);
    return;
  }
  const r = mapa.regionEn(e.clientX, e.clientY);
  if (!r) return;
  foco = r.id;
  ver(r.id, true);
  const l = await lente([r.id], 2, 300);
  mapa.mostrar(l);
  decir(`${r.path} · ${r.weight} archivos · ${l.nodes.length} nodos · Esc para volver`);
});

window.addEventListener("keydown", (e) => {
  if (explorador.visible) return; // el explorador tiene sus propias teclas
  // El escalón de §4.1 que no pasa por la barra: el conmutador **también es una
  // tecla**, disponible en cualquier escalón.
  if (e.key.toLowerCase() === "e" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    void conmutar();
    return;
  }
  if (e.key === "Escape" && document.activeElement !== $consulta) {
    // Escape deshace un escalón, no todos: primero cierra el panel y el mapa se
    // queda donde estaba. Volver al cielo de golpe perdería el sitio al que
    // costó llegar.
    if (panel.visible) panel.ocultar();
    else alCielo();
  }
});

/** Ir a otra entidad desde una línea del panel: se enfoca y se enseña. */
panel.onNavegar = (id) => {
  foco = id;
  ver(id, true);
  if (!explorador.visible) void lente([id], 2, 300).then((l) => mapa.mostrar(l));
};

// ────────────────────────────────────────────────── el conmutador (§4.7)

/**
 * Abre el explorador **donde estabas**, y al volver deja la cámara donde el
 * explorador te dejó. Las dos direcciones son requisito: si una te devolviera
 * al inicio, dejarían de ser dos vistas de un solo estado y pasarían a ser dos
 * aplicaciones compartiendo ventana.
 */
async function conmutar(): Promise<void> {
  if (explorador.visible) {
    explorador.cerrar();
    return;
  }
  mapa.congelar();
  $barra.style.visibility = "hidden";
  $conmutador.style.visibility = "hidden";
  // El panel se muda, no se duplica: dentro del explorador es la columna de
  // detalle (ADR-0014 §6), y sigue siendo el mismo elemento con el mismo estado.
  panel.montar($detalle);
  await explorador.abrir(foco);
}

explorador.onCerrar = (donde) => {
  $barra.style.visibility = "";
  $conmutador.style.visibility = "";
  panel.montar(document.body);
  mapa.descongelar();
  if (donde) {
    foco = donde;
    void lente([donde], 2, 300).then((l) => mapa.mostrar(l));
  }
  $consulta.focus();
};

explorador.onFila = (f) => {
  foco = f.id;
  // Sin consulta: al explorador se llega navegando, no buscando, así que no hay
  // término que centrar y el documento se abre por el principio.
  ver(f.id, false, "");
};

$conmutador.addEventListener("click", () => void conmutar());

void arrancar();
