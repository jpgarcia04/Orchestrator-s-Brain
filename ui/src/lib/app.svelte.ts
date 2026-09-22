/**
 * El estado de la aplicación: **uno solo**, y todas las vistas lo leen.
 *
 * Es la consecuencia #4 de ADR-0014 —dos vistas de un solo estado— llevada al
 * escritorio entero: el explorador, el mapa, el buscador y la vista previa no
 * guardan cada uno su idea de qué está abierto; la leen de aquí.
 */

import {
  actualizar as pedirActualizar,
  alIndexar,
  anadirBiblioteca,
  entidad,
  estado as pedirEstado,
  explorar,
} from "./nucleo";
import type { Estado, Indexado, Listado } from "./nucleo";

export type Vista = "inicio" | "carpeta" | "mapa" | "sensibles";
export type Tema = "noche" | "dia";
export type Modo = "cuadricula" | "lista";

/** Preferencias de este equipo. Si el almacenamiento falla, se usan las de fábrica. */
function leer(clave: string): string | null {
  try {
    return localStorage.getItem(clave);
  } catch {
    return null;
  }
}

function guardar(clave: string, valor: string): void {
  try {
    localStorage.setItem(clave, valor);
  } catch {
    // sin almacenamiento, la preferencia dura lo que la sesión
  }
}

class App {
  vista = $state<Vista>("inicio");
  /** La raíz: tu bóveda (o tus bibliotecas, si hay varias). */
  raiz = $state<Listado | null>(null);
  /** La carpeta abierta en el explorador. */
  listado = $state<Listado | null>(null);
  /** Lo que se enseña en la vista previa. */
  seleccion = $state<string | null>(null);
  /** Lo que se buscó para llegar a la selección: la previa abre por ahí. */
  consulta = $state("");
  modo = $state<Modo>(leer("obrain.modo") === "lista" ? "lista" : "cuadricula");
  tema = $state<Tema>(leer("obrain.tema") === "dia" ? "dia" : "noche");
  buscando = $state(false);
  /** La letra con la que se abrió el buscador escribiendo: no se pierde. */
  semilla = $state("");
  info = $state<Estado | null>(null);
  indexando = $state(false);
  ultimo = $state<Indexado | null>(null);
  /** En el mapa: de quién es la lente, o `null` para el cielo entero. */
  lente = $state<string | null>(null);
  aviso = $state("");
  /** Sube cada vez que termina un reindexado: lo que se pinta con datos vuelve a pedirlos. */
  version = $state(0);
  error = $state("");

  private temporizador: number | undefined;

  /** ¿La bóveda está sin empezar? */
  get vacia(): boolean {
    return this.info !== null && this.info.libraries.length === 0;
  }

  async arrancar(): Promise<void> {
    await alIndexar((i) => void this.terminoIndexado(i));
    await this.refrescar();
  }

  /** Vuelve a pedir lo que depende de la base. */
  async refrescar(): Promise<void> {
    try {
      this.info = await pedirEstado();
      this.indexando = this.info.indexing;
      this.raiz = await explorar(null);
      if (this.vista === "carpeta" && this.listado) {
        try {
          this.listado = await explorar(this.listado.focus);
        } catch {
          // La carpeta abierta ya no está (se movió o se borró): a la raíz.
          this.listado = this.raiz;
          this.seleccion = null;
        }
      }
      this.error = "";
    } catch (e) {
      this.error = `No se pudo abrir la base — ${e}`;
    }
    this.version++;
  }

  private async terminoIndexado(i: Indexado): Promise<void> {
    this.ultimo = i;
    this.indexando = false;
    await this.refrescar();
    if (i.error) {
      this.avisar(`No se pudo actualizar: ${i.error}`);
    } else if (i.report && i.report.files_new > 0) {
      const n = i.report.files_new;
      this.avisar(n === 1 ? "Hay 1 documento nuevo en tu bóveda." : `Hay ${n} documentos nuevos en tu bóveda.`);
    }
  }

  async actualizar(): Promise<void> {
    try {
      if (await pedirActualizar()) this.indexando = true;
    } catch (e) {
      this.avisar(`No se pudo actualizar — ${e}`);
    }
  }

  async anadirCarpeta(): Promise<void> {
    try {
      if (await anadirBiblioteca()) this.indexando = true;
    } catch (e) {
      this.avisar(`No se pudo añadir la carpeta — ${e}`);
    }
  }

  irVista(v: Vista): void {
    this.vista = v;
    this.seleccion = null;
    if (v === "mapa") this.lente = null;
  }

  /** Abre una carpeta en el explorador; `null` es la raíz de la bóveda. */
  async abrirCarpeta(id: string | null, seleccionar: string | null = null, consulta = ""): Promise<void> {
    try {
      this.listado = await explorar(id);
    } catch (e) {
      this.avisar(`No se pudo abrir la carpeta — ${e}`);
      return;
    }
    this.vista = "carpeta";
    this.seleccion = seleccionar;
    this.consulta = consulta;
  }

  /**
   * Llega a un archivo: abre **su carpeta** y lo señala.
   *
   * La carpeta se le pregunta al grafo (`contained_by`) y no a la ruta: es el
   * mismo árbol que dibuja el mapa, así que las dos vistas no pueden discrepar.
   */
  async irA(id: string, consulta = ""): Promise<void> {
    try {
      const v = await entidad(id);
      const casa = v.observed.find((c) => c.predicate === "contained_by")?.value_entity ?? null;
      await this.abrirCarpeta(casa, id, consulta);
    } catch (e) {
      this.avisar(`No se pudo abrir — ${e}`);
    }
  }

  /** Enseña algo en la vista previa sin moverse de donde estás. */
  ver(id: string, consulta = ""): void {
    this.seleccion = id;
    this.consulta = consulta;
  }

  cerrarPanel(): void {
    this.seleccion = null;
  }

  /** Del documento al mapa: su vecindario, con él en el centro. */
  verEnMapa(id: string): void {
    this.lente = id;
    this.vista = "mapa";
  }

  abrirBuscador(semilla = ""): void {
    this.semilla = semilla;
    this.buscando = true;
  }

  alternarTema(): void {
    this.tema = this.tema === "noche" ? "dia" : "noche";
    guardar("obrain.tema", this.tema);
  }

  ponerModo(m: Modo): void {
    this.modo = m;
    guardar("obrain.modo", m);
  }

  avisar(mensaje: string): void {
    this.aviso = mensaje;
    window.clearTimeout(this.temporizador);
    this.temporizador = window.setTimeout(() => (this.aviso = ""), 3200);
  }
}

export const app = new App();
