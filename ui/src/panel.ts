/**
 * El panel de entidad de [`EXPERIENCE.md`] §4.4, **y lo que ese texto nunca
 * dijo: qué se ve del archivo.**
 *
 * El orden no es estético y no se toca: *qué es · observado · inferido con su
 * «¿por qué?» · corrección*. La separación entre los dos bloques centrales es
 * el invariante I9 hecho pantalla —un hash y la sospecha de un clasificador no
 * pueden leerse igual— y la decide el núcleo, no esta hoja.
 *
 * Lo único que se ha añadido a ese orden es la **previsualización**, y va
 * arriba del todo por el motivo que ordena el hito entero: *«si no puedo ver
 * los archivos, no va a tener mucho chiste la app»*. Un panel que enseña
 * facetas y procedencia sobre un archivo que no puedes ver es una ficha
 * catalográfica.
 *
 * ## Un panel, dos sitios
 *
 * Esto se **reparenta**: en la galaxia cuelga del cuerpo y es un lateral que no
 * tapa el mapa (§4.4); dentro del explorador es la columna de detalle
 * ([ADR-0014] §6). Es el mismo elemento y el mismo estado a propósito. Dos
 * implementaciones del panel serían dos sitios donde arreglar el mismo fallo, y
 * la consecuencia #4 del ADR —dos vistas de un solo estado— dejaría de ser
 * cierta justo en la parte que el usuario mira.
 *
 * [`EXPERIENCE.md`]: ../../docs/EXPERIENCE.md
 * [ADR-0014]: ../../docs/adr/0014-dos-vistas-un-solo-estado.md
 */

import { abrir, corregir, entidad, previsualizar, revelar } from "./nucleo";
import type { Claim, Vista, VistaEntidad } from "./nucleo";

/** Predicados cuyo valor es una entidad y no se pueden editar escribiendo. */
const esEnlace = (c: Claim) => c.value_entity !== null;

const seEscribeEnLaNube = (v: Vista) => v.availability !== null && v.availability !== "local";

export class Panel {
  private raiz: HTMLElement;
  private id: string | null = null;
  private consulta = "";
  /** Descarta respuestas que llegan tarde: la flecha del explorador va rápido. */
  private generacion = 0;
  /**
   * Bóvedas que el usuario ha pedido ver en esta sesión.
   *
   * No se recuerda entre arranques a propósito: la decisión de enseñar una
   * contraseña en pantalla se toma cada vez, no una vez.
   */
  private revelado = new Set<string>();

  /** Ir a otra entidad desde una línea del panel. Lo pone `main`. */
  onNavegar: (id: string) => void = () => {};

  constructor(raiz: HTMLElement) {
    this.raiz = raiz;
    this.raiz.addEventListener("click", (e) => void this.pulsar(e));
  }

  get visible(): boolean {
    return !this.raiz.hidden;
  }

  get entidad(): string | null {
    return this.id;
  }

  /** Lo cuelga de donde toque. Ver la nota de arriba sobre reparentar. */
  montar(host: HTMLElement): void {
    if (this.raiz.parentElement !== host) host.append(this.raiz);
    this.marcarCuerpo();
  }

  ocultar(): void {
    this.raiz.hidden = true;
    this.id = null;
    this.generacion++;
    this.marcarCuerpo();
  }

  /**
   * Dice si el panel está **apartando** la barra y el conmutador.
   *
   * Sólo cuando cuelga del cuerpo: dentro del explorador no aparta nada porque
   * la barra está escondida y el conmutador también. Lo decide aquí y no
   * `main` porque es el panel quien sabe dónde está colgado, y repartir ese
   * dato en dos sitios es cómo el conmutador acaba medio tapado.
   */
  private marcarCuerpo(): void {
    const apartando = this.visible && this.raiz.parentElement === document.body;
    document.body.classList.toggle("con-panel", apartando);
  }

  /**
   * Enseña una entidad. `consulta` sólo decide **dónde se abre** el texto.
   */
  async mostrar(id: string, consulta = ""): Promise<void> {
    const mia = ++this.generacion;
    this.id = id;
    this.consulta = consulta;
    this.raiz.hidden = false;

    let ficha: VistaEntidad;
    let vista: Vista;
    try {
      [ficha, vista] = await Promise.all([entidad(id), previsualizar(id, consulta || undefined)]);
    } catch (err) {
      if (mia !== this.generacion) return;
      this.raiz.replaceChildren(aviso(`no se pudo abrir esta entidad — ${err}`));
      return;
    }
    if (mia !== this.generacion) return; // llegó tarde

    this.raiz.replaceChildren(
      this.cabecera(ficha, vista),
      this.previsualizacion(vista),
      this.acciones(vista),
      ...this.hechos(ficha),
    );
    this.raiz.scrollTop = 0;
    this.marcarCuerpo();
  }

  /** Vuelve a pedir todo. Se usa tras una corrección: se **comprueba** que la
   *  línea cambió de bloque, no se simula moviéndola a mano. */
  private async refrescar(): Promise<void> {
    if (this.id) await this.mostrar(this.id, this.consulta);
  }

  // ───────────────────────────────────────────────────────────── los gestos

  private async pulsar(e: MouseEvent): Promise<void> {
    const b = (e.target as HTMLElement).closest<HTMLElement>("[data-accion]");
    if (!b || !this.id) return;
    const id = this.id;

    switch (b.dataset["accion"]) {
      case "cerrar":
        this.ocultar();
        break;
      case "abrir":
        await abrir(id).catch((err) => this.decir(`no se pudo abrir — ${err}`));
        break;
      case "revelar":
        await revelar(id).catch((err) => this.decir(`no se pudo revelar — ${err}`));
        break;
      case "revelar-contenido":
        this.revelado.add(id);
        await this.refrescar();
        break;
      case "ir": {
        const destino = b.dataset["entidad"];
        if (destino) this.onNavegar(destino);
        break;
      }
      case "porque":
        b.closest("li")?.classList.toggle("abierto");
        break;
      case "confirmar": {
        // I5 en un clic: lo que afirma el usuario gana para siempre y ningún
        // reescaneo lo pisa. La línea pasa de pálida a sólida **delante de ti**.
        const { predicado, valor } = b.dataset;
        if (!predicado) break;
        await corregir(id, predicado, valor ?? "")
          .then(() => this.refrescar())
          .catch((err) => this.decir(`no se pudo guardar — ${err}`));
        break;
      }
      case "editar": {
        const li = b.closest("li");
        li?.querySelector<HTMLElement>(".corregir")?.toggleAttribute("hidden");
        li?.querySelector<HTMLInputElement>(".corregir input")?.focus();
        break;
      }
      case "guardar": {
        const li = b.closest("li");
        const predicado = li?.querySelector<HTMLElement>("[data-predicado]")?.dataset["predicado"];
        const valor = li?.querySelector<HTMLInputElement>(".corregir input")?.value.trim();
        if (!predicado || !valor) break;
        await corregir(id, predicado, valor)
          .then(() => this.refrescar())
          .catch((err) => this.decir(`no se pudo guardar — ${err}`));
        break;
      }
    }
  }

  private decir(texto: string): void {
    this.raiz.querySelector(".panel-aviso")?.replaceChildren(texto);
  }

  // ────────────────────────────────────────────────────────────── el dibujo

  private cabecera(f: VistaEntidad, v: Vista): HTMLElement {
    const h = el("header", "panel-cabecera");

    const cerrar = boton("cerrar", "✕");
    cerrar.className = "cerrar";
    cerrar.title = "cerrar el panel";
    h.append(cerrar);

    const titulo = el("h2");
    titulo.textContent = f.label;
    h.append(titulo);

    // Las marcas van arriba y no al final: lo que decide si este archivo se
    // puede tocar tiene que verse antes de tocarlo.
    const marcas = el("div", "marcas");
    if (seEscribeEnLaNube(v)) {
      marcas.append(marca("nube", "☁ en tu nube", "leerlo aquí lo descargaría, así que no se ha tocado"));
    }
    if (f.sensitivity !== "normal") {
      marcas.append(
        marca("boveda", f.sensitivity === "vault" ? "BÓVEDA" : "SENSIBLE",
          "no sale de esta máquina: fuera del índice semántico y de cualquier modelo remoto"),
      );
    }
    if (marcas.childElementCount > 0) h.append(marcas);

    if (f.path) {
      const ruta = el("div", "ruta");
      ruta.textContent = f.path;
      ruta.title = f.path;
      h.append(ruta);
    }

    // §3.2: el «tipo» es una composición de facetas, no una etiqueta rígida.
    if (f.facets.length > 0) {
      const facetas = el("div", "facetas");
      facetas.textContent = f.facets.join(" · ");
      h.append(facetas);
    }

    h.append(el("p", "panel-aviso"));
    return h;
  }

  private previsualizacion(v: Vista): HTMLElement {
    const caja = el("div", "vista");

    // Bóveda: se ve si lo pides, no por pasar la flecha por encima. La puerta
    // de §5.2 gobierna el egreso y no la pantalla, pero que se te revele una
    // contraseña sin pedirlo es una sorpresa que no se quiere delante de nadie.
    // Se pregunta por `this.id` y no por `v.id` aunque el núcleo devuelva
    // siempre el mismo: son dos variables para la misma cosa, y basta con que
    // una vez discrepen para que un archivo en bóveda se enseñe solo.
    if (v.sensitivity !== "normal" && !this.revelado.has(this.id ?? "") && v.kind !== "none") {
      const b = boton("revelar-contenido", "mostrar contenido");
      b.className = "revelar-contenido";
      caja.append(nota("el contenido de este archivo está oculto porque está marcado como sensible"), b);
      return caja;
    }

    switch (v.kind) {
      case "text": {
        caja.append(texto(v));
        const pie = el("div", "pie");
        pie.textContent = v.truncated
          ? `viendo ${(v.text?.length ?? 0).toLocaleString("es")} de ${v.chars.toLocaleString("es")} caracteres`
          : `${v.chars.toLocaleString("es")} caracteres`;
        caja.append(pie);
        break;
      }
      case "image": {
        const img = document.createElement("img");
        img.src = v.image ?? "";
        img.alt = "";
        caja.append(img);
        const pie = el("div", "pie");
        pie.textContent = `${v.width} × ${v.height}`;
        caja.append(pie);
        break;
      }
      default:
        // §4.6: «no lo sé» es una pantalla diseñada. El sistema ya sabía la
        // respuesta —la tiene en el log— y hasta hoy no la estaba diciendo.
        caja.append(nota(v.reason ?? "no hay nada que enseñar de este archivo"));
    }
    return caja;
  }

  private acciones(v: Vista): HTMLElement {
    const caja = el("div", "acciones");
    if (!v.path) return caja;

    const enLaNube = seEscribeEnLaNube(v);
    const b = boton("abrir", enLaNube ? "abrir · lo descargará de tu nube" : "abrir");
    if (enLaNube) {
      b.classList.add("cuidado");
      // La única excepción que la regla de M5b permite: hidratar **sólo** como
      // acto explícito del usuario, y dicho con todas las letras. Éstas.
      b.title = "este archivo es un puntero: abrirlo lo baja de OneDrive a tu disco";
    }
    caja.append(b, boton("revelar", "ver en la carpeta"));
    caja.append(
      nota("obrain no escribe en tus archivos. El programa que lo abra sí puede."),
    );
    return caja;
  }

  private hechos(f: VistaEntidad): HTMLElement[] {
    const out: HTMLElement[] = [];
    if (f.observed.length > 0) {
      out.push(this.bloque("observado", f.observed, false));
    }
    if (f.inferred.length > 0) {
      out.push(this.bloque("inferido", f.inferred, true));
    }
    return out;
  }

  private bloque(titulo: string, claims: Claim[], corregible: boolean): HTMLElement {
    const s = el("section", corregible ? "inferido" : "observado");
    const h = el("h3");
    h.textContent = titulo;
    s.append(h);

    const ul = document.createElement("ul");
    for (const c of claims) {
      ul.append(this.linea(c, corregible));
    }
    s.append(ul);
    return s;
  }

  private linea(c: Claim, corregible: boolean): HTMLElement {
    const li = document.createElement("li");
    // §1: la confianza es saturación. Una inferencia dudosa sale literalmente
    // pálida, y no porque una hoja de estilos lo decida por categoría.
    li.style.setProperty("--confianza", String(Math.max(0.35, c.confidence)));
    if (c.by_user) li.classList.add("del-usuario");

    const cabeza = el("div", "claim");
    const etiqueta = el("span", "predicado");
    etiqueta.dataset["predicado"] = c.predicate;
    etiqueta.textContent = c.label;
    cabeza.append(etiqueta);

    if (esEnlace(c)) {
      const b = boton("ir", c.value);
      b.className = "enlace";
      b.dataset["entidad"] = c.value_entity ?? "";
      cabeza.append(b);
    } else {
      const v = el("span", "valor");
      // Un instante se escribe en la hora de quien mira; el núcleo sólo sabe
      // UTC, y una sesión de esta tarde saldría fechada mañana.
      v.textContent =
        c.value_time !== null
          ? new Date(c.value_time).toLocaleString("es", { dateStyle: "medium", timeStyle: "short" })
          : c.value;
      cabeza.append(v);
    }

    if (c.by_user) {
      // §4.5: la autoridad del usuario tiene que **verse**, y ser permanente.
      cabeza.append(marca("tuyo", "lo dijiste tú", "tu afirmación manda sobre cualquier inferencia, para siempre"));
    }
    li.append(cabeza);

    if (corregible) {
      const acciones = el("div", "claim-acciones");
      if (c.evidence) {
        acciones.append(boton("porque", "¿por qué?"));
      } else {
        const sin = el("span", "sin-prueba");
        sin.textContent = `${c.source} · confianza ${c.confidence.toFixed(2)}`;
        acciones.append(sin);
      }
      const ok = boton("confirmar", "es correcto");
      ok.dataset["predicado"] = c.predicate;
      ok.dataset["valor"] = c.value;
      acciones.append(ok);
      // Sólo lo que es un literal se puede corregir escribiendo. Cambiar a qué
      // entidad apunta una arista necesita elegir la otra entidad, y eso es un
      // selector que este hito no tiene — decirlo es mejor que un campo de
      // texto que guarda una cadena donde tendría que ir un identificador.
      if (!esEnlace(c)) acciones.append(boton("editar", "corregir"));
      li.append(acciones);

      if (c.evidence) {
        const prueba = el("blockquote", "prueba");
        prueba.textContent = c.evidence;
        li.append(prueba);
      }
      if (!esEnlace(c)) {
        const caja = el("div", "corregir");
        caja.hidden = true;
        const input = document.createElement("input");
        input.type = "text";
        input.value = c.value;
        input.setAttribute("aria-label", `corregir ${c.label}`);
        caja.append(input, boton("guardar", "guardar"));
        li.append(caja);
      }
    }
    return li;
  }
}

// ────────────────────────────────────────────────────────────────── ayudas

function el(tag: string, clase?: string): HTMLElement {
  const e = document.createElement(tag);
  if (clase) e.className = clase;
  return e;
}

function boton(accion: string, texto: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.dataset["accion"] = accion;
  b.textContent = texto;
  return b;
}

function marca(clase: string, texto: string, titulo: string): HTMLElement {
  const m = el("span", clase);
  m.textContent = texto;
  m.title = titulo;
  return m;
}

function nota(texto: string): HTMLElement {
  const p = el("p", "nose");
  p.textContent = texto;
  return p;
}

function aviso(texto: string): HTMLElement {
  const p = el("p", "panel-aviso");
  p.textContent = texto;
  return p;
}

/**
 * El texto, con el tramo que explica la consulta marcado.
 *
 * Se construye con nodos de texto y no con `innerHTML`: lo que se está pintando
 * es el contenido de un archivo del usuario, y un documento con `<script>`
 * dentro es un documento perfectamente normal.
 */
function texto(v: Vista): HTMLElement {
  const pre = el("pre", "texto");
  const t = v.text ?? "";
  const marca = v.highlight;
  if (!marca) {
    pre.textContent = t;
    return pre;
  }
  const [s, f] = marca;
  const resaltado = document.createElement("mark");
  resaltado.textContent = t.slice(s, f);
  pre.append(document.createTextNode(t.slice(0, s)), resaltado, document.createTextNode(t.slice(f)));
  // Que la marca se vea sin buscarla: el motivo entero de elegir la ventana
  // (ADR-0016) era que la explicación estaba doscientos caracteres más abajo.
  queueMicrotask(() => resaltado.scrollIntoView({ block: "center" }));
  return pre;
}
