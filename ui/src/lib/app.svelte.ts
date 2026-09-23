/**
 * El estado de la aplicación: **uno solo**, y todas las vistas lo leen.
 *
 * Es la consecuencia #4 de ADR-0014 —dos vistas de un solo estado— llevada al
 * escritorio entero: el explorador, el mapa, el buscador y la vista previa no
 * guardan cada uno su idea de qué está abierto; la leen de aquí.
 */

import {
  actualizar as pedirActualizar,
  alEmpezarIndexado,
  alIndexar,
  anadirBiblioteca,
  entidad,
  estado as pedirEstado,
  explorar,
  marcarCarpeta as pedirMarca,
  pinBloquear,
  seguridad as pedirSeguridad,
} from "./nucleo";
import type { Estado, Indexado, Listado, Seguridad } from "./nucleo";

export type Vista = "inicio" | "carpeta" | "mapa" | "sensibles" | "papelera";
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

/** Los apartados de la vista previa que cambiaste: `id → abierto`. */
function leerApartados(): Record<string, boolean> {
  try {
    const v: unknown = JSON.parse(leer("obrain.apartados") ?? "{}");
    if (v && typeof v === "object" && !Array.isArray(v)) {
      return Object.fromEntries(Object.entries(v).filter(([, b]) => typeof b === "boolean"));
    }
  } catch {
    // un valor roto vale lo mismo que ninguno
  }
  return {};
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
  /** Cifrado y PIN (ADR-0021). `null` hasta saberlo, o con la base cerrada. */
  seguridad = $state<Seguridad | null>(null);
  /** El panel de Seguridad, abierto. */
  panelSeguridad = $state(false);
  /** Se está pidiendo el PIN para enseñar algo sensible. */
  pidiendoPin = $state(false);
  /** «Más tarde» en el aviso de la clave de recuperación: vuelve al abrir la app. */
  recuperacionAplazada = $state(false);
  /**
   * Los apartados de la vista previa que plegaste o desplegaste. Se recuerdan
   * en este equipo, para todos los documentos: lo que no quieres ver no vuelve
   * a salir al abrir otro. Sólo se guarda lo que cambiaste de como nace.
   */
  apartados = $state<Record<string, boolean>>(leerApartados());

  private temporizador: number | undefined;
  private resolverPin: ((ok: boolean) => void) | null = null;

  /** ¿Hay que enseñar la clave de recuperación? Hasta que digas que la guardaste. */
  get mostrarRecuperacion(): boolean {
    return !!this.seguridad?.recuperacion_pendiente && !this.recuperacionAplazada && !this.info?.cerrada;
  }

  /** Hay una ventana encima: las teclas sueltas no abren el buscador. */
  get enModal(): boolean {
    return this.pidiendoPin || this.panelSeguridad || this.mostrarRecuperacion;
  }

  /** ¿La bóveda está sin empezar? */
  get vacia(): boolean {
    return this.info !== null && this.info.libraries.length === 0;
  }

  async arrancar(): Promise<void> {
    await alIndexar((i) => void this.terminoIndexado(i));
    await alEmpezarIndexado(() => (this.indexando = true));
    await this.refrescar();
  }

  /** Vuelve a pedir lo que depende de la base. */
  async refrescar(): Promise<void> {
    try {
      this.info = await pedirEstado();
      this.indexando = this.info.indexing;
      if (this.info.cerrada) {
        // Sin la llave no hay nada más que pedir: la ventana pide la clave.
        this.seguridad = null;
        this.error = "";
        this.version++;
        return;
      }
      await this.recargarSeguridad();
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
    } else if (!i.live && i.report && i.report.files_new > 0) {
      // Lo que encuentra el vigía no se anuncia: aparece y ya (ADR-0024).
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

  /**
   * Marca una carpeta como sensible, o le quita la marca (`null`). Todo lo que
   * depende de la base se vuelve a pedir: la carpeta, sus filas, la vista
   * previa abierta y la lista de Sensibles cambian a la vez.
   */
  async marcarCarpeta(id: string, nombre: string, nivel: "sensitive" | null): Promise<void> {
    try {
      await pedirMarca(id, nivel);
      await this.refrescar();
      if (nivel) {
        this.avisar(`Todo lo de «${nombre}» es sensible desde ahora, también lo que llegue después.`);
      } else {
        this.avisar(`«${nombre}» ya no está marcada. Lo de dentro vuelve a lo que diga su nombre o su texto.`);
      }
    } catch (e) {
      this.avisar(`No se pudo marcar la carpeta — ${e}`);
    }
  }

  async recargarSeguridad(): Promise<void> {
    try {
      this.seguridad = await pedirSeguridad();
    } catch {
      this.seguridad = null;
    }
  }

  /**
   * Pide el PIN si hace falta. Resuelve `true` cuando se puede ver lo
   * sensible —ya estaba desbloqueado, no hay PIN, o se acaba de escribir— y
   * `false` si se canceló.
   */
  pedirPin(): Promise<boolean> {
    if (!this.seguridad?.pin) return Promise.resolve(true);
    return new Promise((resolver) => {
      this.resolverPin?.(false);
      this.resolverPin = resolver;
      this.pidiendoPin = true;
    });
  }

  /** Lo llama la ventana del PIN al terminar. */
  async pinResuelto(ok: boolean): Promise<void> {
    this.pidiendoPin = false;
    const r = this.resolverPin;
    this.resolverPin = null;
    if (ok) {
      await this.recargarSeguridad();
      this.version++; // lo que estaba bloqueado se vuelve a pedir
    }
    r?.(ok);
  }

  /** Vuelve a tapar lo sensible ya, sin esperar a que caduque. */
  async bloquear(): Promise<void> {
    try {
      await pinBloquear();
    } catch {
      // con la base cerrada no hay nada que bloquear
    }
    await this.recargarSeguridad();
    this.version++;
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

  /** ¿Está abierto este apartado de la vista previa? */
  abierto(id: string, deFabrica: boolean): boolean {
    return this.apartados[id] ?? deFabrica;
  }

  /** Abre o pliega un apartado de la vista previa, y lo recuerda. */
  ponerApartado(id: string, abierto: boolean): void {
    this.apartados[id] = abierto;
    guardar("obrain.apartados", JSON.stringify(this.apartados));
  }

  avisar(mensaje: string): void {
    this.aviso = mensaje;
    window.clearTimeout(this.temporizador);
    this.temporizador = window.setTimeout(() => (this.aviso = ""), 3200);
  }
}

export const app = new App();
