/**
 * El explorador de [ADR-0014]: **bajar hasta cualquier nodo sin escribir nada.**
 *
 * Existe porque hasta hoy no había ninguna forma de navegar sin buscar. La
 * lente responde *«esto y lo que tiene al lado»* y necesita un foco previo; el
 * cielo responde *«cómo es el paisaje»* y no tiene pila de navegación. Ninguna
 * de las dos contesta *«enséñame lo que hay dentro de esto»*.
 *
 * ## Es un modo, no un modal
 *
 * `EXPERIENCE.md` §4.4 prohíbe que tocar un nodo haga aparecer algo que tape el
 * mapa. Esto **nunca aparece solo**: lo pide el usuario con el conmutador o con
 * la tecla, y por eso no es lo que §4.4 rechaza. La distinción no es sutil —es
 * la diferencia entre que el sistema te quite el contexto y que lo apartes tú—
 * y en cuanto esto se abra por consecuencia de algo, deja de sostenerse.
 *
 * ## Columnas, y por qué no una sola lista
 *
 * Una jerarquía profunda tiene una representación que lleva cuarenta años
 * ganando y son las columnas: se ve dónde estás, de dónde vienes y qué había al
 * lado, sin recordarlo. Una sola lista con migas obliga a reconstruir de memoria
 * los dos niveles de arriba en cada paso.
 *
 * [ADR-0014]: ../../docs/adr/0014-dos-vistas-un-solo-estado.md
 */

import { entidad, explorar } from "./nucleo";
import type { Fila, Listado } from "./nucleo";

/** Lo que se puede leer sin descargar nada. Ver `Availability::may_read_content`. */
export const sePuedeLeer = (f: Fila) => f.availability === "local";

const fecha = (ms: number) =>
  ms > 0 ? new Date(ms).toLocaleDateString("es", { year: "numeric", month: "short", day: "numeric" }) : "";

/** Un nivel abierto: lo que se pidió y qué fila está señalada. */
interface Nivel {
  listado: Listado;
  seleccion: number;
}

export class Explorador {
  private raiz: HTMLElement;
  private $migas: HTMLElement;
  private $columnas: HTMLElement;
  private niveles: Nivel[] = [];
  private abierto = false;

  /** Qué hacer cuando el usuario señala un archivo. Lo pone `main`. */
  onFila: (fila: Fila) => void = () => {};
  /** Al cerrar: dónde estaba, para que la cámara no vuelva al inicio. */
  onCerrar: (foco: string | null) => void = () => {};

  constructor(raiz: HTMLElement) {
    this.raiz = raiz;
    this.$migas = raiz.querySelector("#migas")!;
    this.$columnas = raiz.querySelector("#columnas")!;

    this.raiz.addEventListener("keydown", (e) => this.tecla(e));
    this.$columnas.addEventListener("click", (e) => {
      const li = (e.target as HTMLElement).closest<HTMLElement>("li[data-fila]");
      if (!li) return;
      const col = Number(li.dataset["columna"]);
      const i = Number(li.dataset["fila"]);
      this.señalar(col, i);
      const f = this.niveles[col]?.listado.rows[i];
      if (f?.kind === "folder") void this.bajar(col, f);
    });
    this.$migas.addEventListener("click", (e) => {
      const b = (e.target as HTMLElement).closest<HTMLElement>("[data-miga]");
      if (b) void this.abrir(b.dataset["miga"] || null);
    });
  }

  get visible(): boolean {
    return this.abierto;
  }

  /** Dónde está ahora mismo. Es la mitad de la continuidad del ADR. */
  get foco(): string | null {
    for (let i = this.niveles.length - 1; i >= 0; i--) {
      const n = this.niveles[i]!;
      const f = n.listado.rows[n.seleccion];
      if (f) return f.id;
      if (n.listado.focus) return n.listado.focus;
    }
    return null;
  }

  /**
   * Abre en el nodo que se le diga. **Requisito, no mejora**: si el conmutador
   * te devolviera siempre a la raíz, esto y la galaxia serían dos aplicaciones
   * compartiendo ventana, y el usuario acabaría eligiendo una para siempre.
   *
   * Si el foco no contiene nada —un archivo— se abre en **su carpeta** y se
   * señala él. Llegar a un archivo y que la ventana aparezca vacía sería
   * técnicamente correcto y una respuesta inútil.
   */
  async abrir(foco: string | null = null): Promise<void> {
    this.raiz.hidden = false;
    this.abierto = true;
    this.raiz.focus();

    let l = await explorar(foco);
    let señalado: string | null = null;
    if (foco && l.rows.length === 0 && l.trail.length === 0) {
      const dentro = await this.contenedorDe(foco);
      if (dentro) {
        señalado = foco;
        l = await explorar(dentro);
      }
    }

    // Las migas del listado son los niveles de arriba. Se piden todos para que
    // las columnas enseñen el camino entero y no sólo el último escalón.
    this.niveles = [];
    for (const m of l.trail) {
      this.niveles.push({ listado: await explorar(m.id), seleccion: -1 });
    }
    this.niveles.push({ listado: l, seleccion: -1 });

    // Cada columna señala la que la abrió, para que el camino se vea marcado.
    for (let i = 0; i < this.niveles.length - 1; i++) {
      const hijo = this.niveles[i + 1]!.listado.focus;
      const j = this.niveles[i]!.listado.rows.findIndex((r) => r.id === hijo);
      if (j >= 0) this.niveles[i]!.seleccion = j;
    }
    const ultimo = this.niveles[this.niveles.length - 1]!;
    ultimo.seleccion = señalado
      ? ultimo.listado.rows.findIndex((r) => r.id === señalado)
      : ultimo.listado.rows.length > 0
        ? 0
        : -1;

    this.pintar();
    const f = ultimo.listado.rows[ultimo.seleccion];
    if (f) this.onFila(f);
  }

  cerrar(): void {
    if (!this.abierto) return;
    this.abierto = false;
    this.raiz.hidden = true;
    this.onCerrar(this.foco);
  }

  /** La carpeta que contiene a esto, según el grafo. */
  private async contenedorDe(id: string): Promise<string | null> {
    try {
      const v = await entidad(id);
      return v.observed.find((c) => c.predicate === "contained_by")?.value_entity ?? null;
    } catch {
      return null;
    }
  }

  private async bajar(columna: number, fila: Fila): Promise<void> {
    const l = await explorar(fila.id);
    this.niveles.length = columna + 1;
    this.niveles.push({ listado: l, seleccion: l.rows.length > 0 ? 0 : -1 });
    this.pintar();
    const f = l.rows[0];
    if (f) this.onFila(f);
  }

  private señalar(columna: number, i: number): void {
    const n = this.niveles[columna];
    if (!n) return;
    n.seleccion = i;
    // Señalar en una columna invalida lo que había a su derecha: ya no cuelga
    // de lo señalado.
    if (this.niveles.length > columna + 1) this.niveles.length = columna + 1;
    this.pintar();
    const f = n.listado.rows[i];
    if (f) this.onFila(f);
  }

  /** La columna donde está el cursor: la última con algo señalado. */
  private get activa(): number {
    return this.niveles.length - 1;
  }

  private tecla(e: KeyboardEvent): void {
    const col = this.activa;
    const n = this.niveles[col];
    if (!n) return;
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        this.cerrar();
        break;
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        const d = e.key === "ArrowDown" ? 1 : -1;
        const total = n.listado.rows.length;
        if (total === 0) return;
        this.señalar(col, (n.seleccion + d + total) % total);
        break;
      }
      case "ArrowRight":
      case "Enter": {
        e.preventDefault();
        const f = n.listado.rows[n.seleccion];
        if (f?.kind === "folder") void this.bajar(col, f);
        break;
      }
      case "ArrowLeft":
        e.preventDefault();
        if (this.niveles.length > 1) {
          this.niveles.pop();
          this.pintar();
        }
        break;
    }
  }

  // ─────────────────────────────────────────────────────────────── el dibujo

  private pintar(): void {
    this.pintarMigas();
    this.$columnas.replaceChildren(
      ...this.niveles.map((n, i) => this.columna(n, i)),
    );
    this.$columnas.scrollLeft = this.$columnas.scrollWidth;
    this.$columnas
      .querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }

  private pintarMigas(): void {
    const partes: HTMLElement[] = [];
    const raiz = document.createElement("button");
    raiz.dataset["miga"] = "";
    raiz.textContent = "inicio";
    partes.push(raiz);
    for (const n of this.niveles) {
      const f = n.listado.focus;
      if (!f) continue;
      const b = document.createElement("button");
      b.dataset["miga"] = f;
      b.textContent = n.listado.label;
      partes.push(b);
    }
    this.$migas.replaceChildren(...partes);
  }

  private columna(n: Nivel, indice: number): HTMLElement {
    const div = document.createElement("div");
    div.className = "columna";
    const ul = document.createElement("ul");

    if (n.listado.rows.length === 0) {
      const vacio = document.createElement("li");
      vacio.className = "vacio";
      // §4.6: «no lo sé» es una pantalla diseñada. Una carpeta vacía en el mapa
      // puede ser una carpeta vacía de verdad o una que el escaneo podó, y hoy
      // el sistema no distingue las dos. Se dice lo que se sabe.
      vacio.textContent = "aquí no hay nada indexado";
      ul.append(vacio);
    }

    n.listado.rows.forEach((f, i) => {
      const li = document.createElement("li");
      li.className = f.kind === "folder" ? "carpeta" : "archivo";
      li.dataset["columna"] = String(indice);
      li.dataset["fila"] = String(i);
      li.setAttribute("aria-selected", i === n.seleccion ? "true" : "false");

      const nombre = document.createElement("span");
      nombre.className = "fila-nombre";
      nombre.textContent = f.label;
      li.append(nombre);

      // Lo que un gestor de archivos no puede enseñar, que es el motivo entero
      // de que esto navegue el grafo y no el disco.
      const meta = document.createElement("span");
      meta.className = "fila-meta";
      if (f.kind === "folder") {
        meta.textContent = f.weight === 1 ? "1" : String(f.weight);
      } else {
        meta.textContent = fecha(f.recency);
      }
      li.append(meta);

      // La nube se marca **en la fila**: es lo que decide si esto se puede
      // previsualizar sin descargarlo, y esconderlo hasta el panel sería
      // enterarte de que tocaste tu OneDrive después de tocarlo.
      if (f.availability !== null && f.availability !== "local") {
        const nube = document.createElement("span");
        nube.className = "nube";
        nube.textContent = "☁";
        nube.title = "está en la nube · leerlo lo descargaría";
        li.append(nube);
      }
      if (f.sensitivity !== "normal") {
        const marca = document.createElement("span");
        marca.className = "boveda";
        marca.textContent = f.sensitivity === "vault" ? "BÓVEDA" : "SENSIBLE";
        li.append(marca);
      }
      ul.append(li);
    });

    div.append(ul);
    return div;
  }
}
