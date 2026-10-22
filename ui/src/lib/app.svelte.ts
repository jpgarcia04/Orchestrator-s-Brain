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
  alRestaurar,
  reanudarReindexado,
  borrarColeccion as pedirBorrarColeccion,
  coleccion as pedirColeccion,
  colecciones as pedirColecciones,
  crearColeccion as pedirCrearColeccion,
  entidad,
  estado as pedirEstado,
  explorar,
  invalidarLecturas,
  marcarCarpeta as pedirMarca,
  meterEnColeccion,
  moverEntreColecciones,
  pinBloquear,
  renombrarColeccion as pedirRenombrar,
  sacarDeColeccion,
  seguridad as pedirSeguridad,
} from "./nucleo";
import type { Abierta, Coleccion, Estado, Indexado, Listado, Seguridad } from "./nucleo";
import { tituloDe } from "./nombres";
import { pinSession } from "./pin-session";

export type Vista = "inicio" | "carpeta" | "mapa" | "sensibles" | "papelera" | "colecciones" | "coleccion";
export type Tema = "noche" | "dia";
export type Modo = "cuadricula" | "lista";

/**
 * Un arrastre hacia una colección de la barra lateral (ADR-0033). Desde una
 * carpeta añade; desde una colección mueve, y con Ctrl añade.
 */
export interface Arrastre {
  /** Lo que se lleva: el documento agarrado, o todos los elegidos si era uno de ellos. */
  docs: string[];
  /** Cómo se llama lo que se lleva: su título, o «3 documentos». */
  etiqueta: string;
  /** La colección de la que sale, si se arrastra desde una colección abierta. */
  desde: string | null;
  /** Dónde está el ratón, en la ventana. */
  x: number;
  y: number;
  /** Ctrl pulsado: añadir aunque venga de una colección. */
  copiar: boolean;
  /** Dónde caería: el id de una colección, `"nueva"`, o ningún sitio. */
  sobre: string | null;
}

/** Soltado en «Nueva colección»: le falta el nombre. */
export interface Pendiente {
  docs: string[];
  etiqueta: string;
  desde: string | null;
  copiar: boolean;
}

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
  /**
   * Un sitio exacto del texto por donde abrir la previa, marcado: dónde está
   * un documento dentro de su paquete (ADR-0030). Manda sobre la consulta.
   */
  tramo = $state<[number, number] | null>(null);
  /** Versión publicada de un pasaje elegido en la búsqueda. */
  expectedHash = $state<string | null>(null);
  expectedTextHash = $state<string | null>(null);
  /** Cada elección explícita invalida una marca interna anterior, incluso en el mismo documento. */
  seleccionVersion = $state(0);
  /** Sólo guarda la intención de consultar citas, nunca su contenido. */
  pasajeAbierto = $state<{ entity: string; query: string } | null>(null);
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
  /**
   * En el mapa: la colección cuyas estrellas se encienden (ADR-0031). El cielo
   * es el de siempre; no se mueve ninguna.
   */
  lenteColeccion = $state<string | null>(null);
  /** Tus colecciones, por nombre (ADR-0031). */
  colecciones = $state<Coleccion[]>([]);
  /** La colección abierta. */
  abierta = $state<Abierta | null>(null);
  /**
   * Los documentos elegidos a la vez (Ctrl+clic, Mayús+clic, Ctrl+A), en el
   * explorador o en una colección. Un clic normal sigue siendo la vista previa.
   */
  elegidos = $state<string[]>([]);
  /** Con el buscador abierto desde una colección: buscar sólo dentro. */
  buscarEn = $state<string | null>(null);
  /** Hay un menú abierto (el de las colecciones): sus teclas son suyas. */
  menu = $state(false);
  /** Un arrastre en curso hacia una colección (ADR-0033). */
  arrastre = $state<Arrastre | null>(null);
  /** Lo soltado en «Nueva colección», esperando su nombre. */
  pendiente = $state<Pendiente | null>(null);
  aviso = $state("");
  /** Sube cada vez que termina un reindexado: lo que se pinta con datos vuelve a pedirlos. */
  version = $state(0);
  /** Ninguna vista documental se monta mientras se retiran datos de una indexación. */
  presentacionLista = $state(false);
  error = $state("");
  /** Cifrado y PIN (ADR-0021). `null` hasta saberlo, o con la base cerrada. */
  seguridad = $state<Seguridad | null>(null);
  securityEpoch = $state(0);
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
  /** Desde dónde cuenta un Mayús+clic: el último documento pulsado. */
  private ancla: string | null = null;
  private resolverPin: ((ok: boolean) => void) | null = null;
  private datosEpoch = 0;
  private focoIndexado: string | null = null;
  private coleccionIndexada: string | null = null;

  constructor() {
    pinSession.subscribe(() => {
      this.seguridad = pinSession.state as Seguridad | null;
      this.securityEpoch = pinSession.epoch;
      this.pasajeAbierto = null;
      this.version++;
    });
  }

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
    await alEmpezarIndexado(() => this.empezoIndexado());
    await alRestaurar(async () => {
      this.avisar("Perfil restaurado. La sincronización automática está en pausa.");
      this.empezoIndexado();
      await this.refrescar();
    });
    await this.refrescar();
  }

  /** Retira selecciones y respuestas anteriores antes de cambiar el inventario. */
  empezoIndexado(): void {
    if (this.presentacionLista) {
      this.focoIndexado = this.vista === "carpeta" ? (this.listado?.focus ?? null) : null;
      this.coleccionIndexada = this.vista === "coleccion" ? (this.abierta?.id ?? null) : null;
    }
    this.indexando = true;
    this.presentacionLista = false;
    this.datosEpoch++;
    invalidarLecturas();
    this.seleccion = null;
    this.elegidos = [];
    this.ancla = null;
    this.arrastre = null;
    this.pendiente = null;
    this.lente = null;
    this.lenteColeccion = null;
    this.buscando = false;
    this.pasajeAbierto = null;
    this.resolverPin?.(false);
    this.resolverPin = null;
    this.pidiendoPin = false;
    this.raiz = null;
    this.listado = null;
    this.abierta = null;
    this.colecciones = [];
  }

  vigilarSeguridad(): () => void {
    const poll = () => { if (!this.info?.cerrada) void this.recargarSeguridad(); };
    const timer = window.setInterval(poll, 1000);
    window.addEventListener("focus", poll);
    document.addEventListener("visibilitychange", poll);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", poll);
      document.removeEventListener("visibilitychange", poll);
    };
  }

  /** Vuelve a pedir lo que depende de la base. */
  async refrescar(): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      const info = await pedirEstado();
      if (epoch !== this.datosEpoch) return;
      this.info = info;
      this.indexando = this.info.indexing;
      if (this.info.cerrada) {
        // Sin la llave no hay nada más que pedir: la ventana pide la clave.
        this.seguridad = null;
        this.error = "";
        this.version++;
        return;
      }
      await this.recargarSeguridad();
      if (epoch !== this.datosEpoch) return;
      this.raiz = await explorar(null);
      if (epoch !== this.datosEpoch) return;
      await this.recargarColecciones();
      if (epoch !== this.datosEpoch) return;
      if (this.vista === "carpeta") {
        try {
          this.listado = await explorar(this.listado?.focus ?? this.focoIndexado);
          if (epoch !== this.datosEpoch) return;
        } catch {
          // La carpeta abierta ya no está (se movió o se borró): a la raíz.
          this.listado = this.raiz;
          this.seleccion = null;
        }
      } else if (this.vista === "coleccion" && this.coleccionIndexada) {
        try {
          this.abierta = await pedirColeccion(this.coleccionIndexada);
          if (epoch !== this.datosEpoch) return;
        } catch {
          this.vista = "colecciones";
        }
      }
      this.focoIndexado = null;
      this.coleccionIndexada = null;
      this.error = "";
      this.presentacionLista = !this.info.indexing;
    } catch (e) {
      if (epoch !== this.datosEpoch) return;
      this.error = `No se pudo abrir la base — ${e}`;
    }
    if (epoch === this.datosEpoch) this.version++;
  }

  private async terminoIndexado(i: Indexado): Promise<void> {
    this.empezoIndexado();
    this.ultimo = i;
    this.indexando = false;
    await this.refrescar();
    if (i.error) {
      this.avisar(`No se pudo actualizar: ${i.error}`);
    } else if (i.report?.errors.length || i.report?.missing.length) {
      this.avisar(i.report.pending > 0
        ? `${i.report.pending} documento(s) siguen protegidos: no se completó la lectura o clasificación. Se reintentará al actualizar.`
        : "Actualización aplazada: falta una biblioteca o hubo un error de lectura. Vuelve a intentarlo cuando esté disponible.");
    } else if (i.report?.deferred.length) {
      this.avisar(`${i.report.deferred.length} documento(s) pendientes: no se pudo confirmar su identidad. Se reintentará al actualizar.`);
    } else if (i.report?.pending) {
      this.avisar(`${i.report.pending} documento(s) siguen protegidos hasta completar su clasificación. Se reintentará al actualizar.`);
    } else if (!i.live && i.report && i.report.files_new > 0) {
      // Lo que encuentra el vigía no se anuncia: aparece y ya (ADR-0024).
      const n = i.report.files_new;
      this.avisar(n === 1 ? "Hay 1 documento nuevo en tu bóveda." : `Hay ${n} documentos nuevos en tu bóveda.`);
    }
  }

  async actualizar(): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      if (await pedirActualizar() && epoch === this.datosEpoch) this.empezoIndexado();
    } catch (e) {
      this.avisar(`No se pudo actualizar — ${e}`);
    }
  }

  async anadirCarpeta(): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      if (await anadirBiblioteca() && epoch === this.datosEpoch) this.empezoIndexado();
    } catch (e) {
      this.avisar(`No se pudo añadir la carpeta — ${e}`);
    }
  }

  async reanudarSincronizacion(): Promise<void> {
    try {
      await reanudarReindexado();
      await this.recargarSeguridad();
      this.avisar("Sincronización de bibliotecas reanudada.");
      this.version++;
    } catch (e) {
      this.avisar(`No se pudo reanudar la sincronización — ${e}`);
    }
  }

  irVista(v: Vista): void {
    this.vista = v;
    this.seleccion = null;
    this.elegidos = [];
    if (v === "mapa") {
      this.lente = null;
      this.lenteColeccion = null;
    }
  }

  /** Abre una carpeta en el explorador; `null` es la raíz de la bóveda. */
  async abrirCarpeta(id: string | null, seleccionar: string | null = null, consulta = "", tramo: [number, number] | null = null, expectedHash: string | null = null, expectedTextHash: string | null = null): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      const listado = await explorar(id);
      if (epoch !== this.datosEpoch || !this.presentacionLista) return;
      this.listado = listado;
    } catch (e) {
      this.avisar(`No se pudo abrir la carpeta — ${e}`);
      return;
    }
    this.vista = "carpeta";
    this.seleccion = seleccionar;
    this.consulta = consulta;
    this.tramo = tramo;
    this.expectedHash = expectedHash;
    this.expectedTextHash = expectedTextHash;
    this.seleccionVersion++;
    this.elegidos = [];
  }

  /**
   * Llega a un archivo: abre **su carpeta** y lo señala.
   *
   * La carpeta se le pregunta al grafo (`contained_by`) y no a la ruta: es el
   * mismo árbol que dibuja el mapa, así que las dos vistas no pueden discrepar.
   */
  async irA(id: string, consulta = "", tramo: [number, number] | null = null, expectedHash: string | null = null, expectedTextHash: string | null = null): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      const v = await entidad(id);
      if (epoch !== this.datosEpoch || !this.presentacionLista) return;
      const casa = v.observed.find((c) => c.predicate === "contained_by")?.value_entity ?? null;
      await this.abrirCarpeta(casa, id, consulta, tramo, expectedHash, expectedTextHash);
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
      this.seguridad = pinSession.state as Seguridad | null;
    }
  }

  /**
   * Pide el PIN si hace falta. Resuelve `true` cuando se puede ver lo
   * sensible —ya estaba desbloqueado, no hay PIN, o se acaba de escribir— y
   * `false` si se canceló.
   */
  pedirPin(): Promise<boolean> {
    if (!this.seguridad) return Promise.resolve(false);
    if (!this.seguridad.pin) return Promise.resolve(true);
    if (this.seguridad.desbloqueada) return Promise.resolve(true);
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
    r?.(ok && this.seguridad !== null && (!this.seguridad.pin || this.seguridad.desbloqueada));
  }

  /** Vuelve a tapar lo sensible ya, sin esperar a que caduque. */
  async bloquear(): Promise<void> {
    try {
      await pinBloquear();
    } catch {
      this.avisar("No se pudo confirmar el bloqueo. El contenido se ha retirado de esta ventana.");
      return;
    }
    await this.recargarSeguridad();
    this.version++;
  }

  /**
   * Enseña algo en la vista previa sin moverse de donde estás. Con `tramo`,
   * abierto por ese sitio de su texto y marcado.
   */
  ver(id: string, consulta = "", tramo: [number, number] | null = null, expectedHash: string | null = null, expectedTextHash: string | null = null): void {
    if (!this.presentacionLista) return;
    this.seleccion = id;
    this.consulta = consulta;
    this.tramo = tramo;
    this.expectedHash = expectedHash;
    this.expectedTextHash = expectedTextHash;
    this.seleccionVersion++;
  }

  cerrarPanel(): void {
    this.seleccion = null;
  }

  /** ¿Hay una lista donde elegir varios? El explorador o una colección. */
  get enLista(): boolean {
    return this.vista === "carpeta" || this.vista === "coleccion";
  }

  /** Los documentos de la lista abierta, en el orden en que se ven. */
  get ordenLista(): string[] {
    if (this.vista === "carpeta") return (this.listado?.rows ?? []).filter((r) => r.kind === "file").map((r) => r.id);
    if (this.vista === "coleccion") return (this.abierta?.rows ?? []).map((r) => r.id);
    return [];
  }

  /** Del documento al mapa: su vecindario, con él en el centro. */
  verEnMapa(id: string): void {
    this.lente = id;
    this.lenteColeccion = null;
    this.vista = "mapa";
  }

  // ─────────────────────────────────────────── tus colecciones (ADR-0031)

  async recargarColecciones(): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      const colecciones = await pedirColecciones();
      if (epoch !== this.datosEpoch) return;
      this.colecciones = colecciones;
      if (this.vista === "coleccion" && this.abierta) {
        try {
          const abierta = await pedirColeccion(this.abierta.id);
          if (epoch !== this.datosEpoch) return;
          this.abierta = abierta;
        } catch {
          // La borraste (quizá desde la terminal): a la lista.
          this.abierta = null;
          this.vista = "colecciones";
        }
      }
    } catch {
      if (epoch !== this.datosEpoch) return;
      this.colecciones = [];
    }
  }

  /** Abre una colección: se ve como una carpeta. */
  async abrirColeccion(id: string, seleccionar: string | null = null): Promise<void> {
    const epoch = this.datosEpoch;
    try {
      const abierta = await pedirColeccion(id);
      if (epoch !== this.datosEpoch || !this.presentacionLista) return;
      this.abierta = abierta;
    } catch (e) {
      this.avisar(`No se pudo abrir la colección — ${e}`);
      return;
    }
    this.vista = "coleccion";
    this.seleccion = seleccionar;
    this.consulta = "";
    this.tramo = null;
    this.expectedHash = null;
    this.expectedTextHash = null;
    this.elegidos = [];
  }

  /** Crea una colección y, si se dan, mete esos documentos. Devuelve su id. */
  async crearColeccion(nombre: string, docs: string[] = []): Promise<string | null> {
    try {
      const id = await pedirCrearColeccion(nombre);
      if (docs.length) await meterEnColeccion(id, docs);
      await this.recargarColecciones();
      const cuantos = docs.length === 0 ? "" : docs.length === 1 ? " con 1 documento" : ` con ${docs.length} documentos`;
      this.avisar(`Colección «${nombre.trim()}» creada${cuantos}.`);
      this.version++;
      return id;
    } catch (e) {
      this.avisar(`No se pudo crear la colección — ${e}`);
      return null;
    }
  }

  /** Mete documentos en una colección. Nada se mueve de su carpeta. */
  async meter(id: string, docs: string[]): Promise<void> {
    const nombre = this.colecciones.find((c) => c.id === id)?.name ?? "la colección";
    try {
      const n = await meterEnColeccion(id, docs);
      // Hecho lo que se quería con lo elegido: se suelta, y el aviso no queda
      // debajo de la barra.
      if (docs.length > 1) this.elegidos = [];
      await this.recargarColecciones();
      this.version++;
      const ya = docs.length - n;
      if (n === 0) this.avisar(docs.length === 1 ? `Ya estaba en «${nombre}».` : `Ya estaban en «${nombre}».`);
      else if (docs.length === 1) this.avisar(`Añadido a «${nombre}».`);
      else
        this.avisar(
          `${n === 1 ? "1 documento añadido" : `${n} documentos añadidos`} a «${nombre}»${ya === 0 ? "" : ya === 1 ? "; el otro ya estaba" : `; ${ya} ya estaban`}.`,
        );
    } catch (e) {
      this.avisar(`No se pudo añadir — ${e}`);
    }
  }

  /** Saca documentos de una colección. Siguen en sus carpetas. */
  async sacar(id: string, docs: string[], etiqueta = ""): Promise<void> {
    const nombre = this.colecciones.find((c) => c.id === id)?.name ?? "la colección";
    try {
      const n = await sacarDeColeccion(id, docs);
      if (this.seleccion && docs.includes(this.seleccion) && this.vista === "coleccion") this.seleccion = null;
      this.elegidos = this.elegidos.filter((e) => !docs.includes(e));
      await this.recargarColecciones();
      this.version++;
      const quien = n === 1 && etiqueta ? `«${tituloDe(etiqueta).titulo}»` : n === 1 ? "1 documento" : `${n} documentos`;
      this.avisar(`${quien} fuera de «${nombre}». ${n === 1 ? "Sigue en su carpeta." : "Siguen en sus carpetas."}`);
    } catch (e) {
      this.avisar(`No se pudo sacar — ${e}`);
    }
  }

  async renombrarColeccion(id: string, nombre: string): Promise<boolean> {
    try {
      await pedirRenombrar(id, nombre);
      await this.recargarColecciones();
      this.version++;
      return true;
    } catch (e) {
      this.avisar(`No se pudo renombrar — ${e}`);
      return false;
    }
  }

  /** Borra una colección. Tus documentos no se tocan. */
  async borrarColeccion(id: string): Promise<void> {
    const nombre = this.colecciones.find((c) => c.id === id)?.name ?? "";
    try {
      await pedirBorrarColeccion(id);
      if (this.abierta?.id === id) {
        this.abierta = null;
        this.vista = "colecciones";
        this.seleccion = null;
      }
      if (this.lenteColeccion === id) this.lenteColeccion = null;
      await this.recargarColecciones();
      this.version++;
      this.avisar(`Colección «${nombre}» borrada. Tus documentos siguen en sus carpetas.`);
    } catch (e) {
      this.avisar(`No se pudo borrar — ${e}`);
    }
  }

  /** De una colección al mapa: el cielo de siempre con sus estrellas encendidas. */
  verColeccionEnMapa(id: string): void {
    this.lente = null;
    this.lenteColeccion = id;
    this.vista = "mapa";
    this.seleccion = null;
  }

  // ─────────────────────────────────────────── elegir varios a la vez

  /**
   * Un clic sobre un documento de una lista. Sin teclas, la vista previa (y
   * se suelta lo elegido); con Ctrl, suma o quita ése; con Mayús, suma el
   * tramo desde el último elegido. `orden` es la lista tal como se ve.
   */
  pulsar(id: string, e: MouseEvent, orden: string[]): void {
    if (e.ctrlKey || e.metaKey) {
      if (this.elegidos.length === 0 && this.seleccion && this.seleccion !== id) this.elegidos = [this.seleccion];
      this.elegidos = this.elegidos.includes(id) ? this.elegidos.filter((x) => x !== id) : [...this.elegidos, id];
      this.ancla = id;
      return;
    }
    if (e.shiftKey) {
      const desde = this.ancla ?? this.seleccion ?? id;
      const a = orden.indexOf(desde);
      const b = orden.indexOf(id);
      if (a >= 0 && b >= 0) {
        const tramo = orden.slice(Math.min(a, b), Math.max(a, b) + 1);
        this.elegidos = [...new Set([...this.elegidos, ...tramo])];
        return;
      }
    }
    this.elegidos = [];
    this.ancla = id;
    this.ver(id);
  }

  /** Todos los de la lista (Ctrl+A). */
  elegirTodos(orden: string[]): void {
    this.elegidos = [...orden];
  }

  soltarElegidos(): void {
    this.elegidos = [];
  }

  // ─────────────────────────────────────────── arrastrar a una colección (ADR-0033)

  /**
   * Empieza a arrastrar `id`. Si es uno de los elegidos, van todos; si no,
   * sólo él, y lo elegido no cambia. Desde una colección abierta, mueve.
   */
  empezarArrastre(id: string, titulo: string, x: number, y: number, copiar: boolean): void {
    const docs = this.elegidos.includes(id) ? [...this.elegidos] : [id];
    this.pendiente = null;
    this.arrastre = {
      docs,
      etiqueta: docs.length === 1 ? titulo : `${docs.length} documentos`,
      desde: this.vista === "coleccion" ? (this.abierta?.id ?? null) : null,
      x,
      y,
      copiar,
      sobre: null,
    };
  }

  /** El ratón se movió, o se pulsó o soltó Ctrl. Sin arrastre (se canceló), nada. */
  moverArrastre(x: number, y: number, copiar: boolean, sobre: string | null): void {
    const a = this.arrastre;
    if (!a) return;
    a.x = x;
    a.y = y;
    a.copiar = copiar;
    a.sobre = sobre;
  }

  /** Cambia sólo Ctrl, sin mover el ratón. */
  copiarArrastre(copiar: boolean): void {
    if (this.arrastre) this.arrastre.copiar = copiar;
  }

  cancelarArrastre(): void {
    this.arrastre = null;
  }

  /** ¿Lo que se lleva sale de su colección al soltarlo? */
  mueve(a: { desde: string | null; copiar: boolean }): boolean {
    return a.desde !== null && !a.copiar;
  }

  /** Lo que pasará si sueltas ahí: lo dice la etiqueta que sigue al ratón. */
  get queHara(): { texto: string; vale: boolean } {
    const a = this.arrastre;
    if (!a || a.sobre === null) return { texto: "Suelta en una colección de la barra lateral", vale: false };
    const uno = a.docs.length === 1;
    if (a.sobre === "nueva") {
      return {
        texto: this.mueve(a) ? "Mover a una colección nueva" : uno ? "Crear una colección con él" : "Crear una colección con ellos",
        vale: true,
      };
    }
    const nombre = this.colecciones.find((c) => c.id === a.sobre)?.name ?? "";
    if (a.sobre === a.desde) return { texto: `${uno ? "Ya está" : "Ya están"} en «${nombre}»`, vale: false };
    return { texto: `${this.mueve(a) ? "Mover" : "Añadir"} a «${nombre}»`, vale: true };
  }

  /** Suelta: en una colección, añade o mueve; en «Nueva colección», pide el nombre. */
  async soltarArrastre(): Promise<void> {
    const a = this.arrastre;
    this.arrastre = null;
    if (!a || a.sobre === null || a.sobre === a.desde) return;
    if (a.sobre === "nueva") {
      this.pendiente = { docs: a.docs, etiqueta: a.etiqueta, desde: a.desde, copiar: a.copiar };
      return;
    }
    if (this.mueve(a) && a.desde) await this.moverA(a.desde, a.sobre, a.docs, a.etiqueta);
    else await this.meter(a.sobre, a.docs);
  }

  /** Crea la colección con lo que se soltó en «Nueva colección». `false` si no se pudo. */
  async crearPendiente(nombre: string): Promise<boolean> {
    const p = this.pendiente;
    if (!p) return false;
    const mueve = this.mueve(p);
    const id = await this.crearColeccion(nombre, mueve ? [] : p.docs);
    if (!id) return false;
    this.pendiente = null;
    if (mueve && p.desde) await this.moverA(p.desde, id, p.docs, p.etiqueta);
    else if (p.docs.length > 1) this.elegidos = [];
    return true;
  }

  /**
   * Mueve documentos de una colección a otra, de una vez. Siguen en sus
   * carpetas: en Windows no cambia nada, y el aviso lo dice.
   */
  async moverA(desde: string, hacia: string, docs: string[], etiqueta: string): Promise<void> {
    const de = this.colecciones.find((c) => c.id === desde)?.name ?? "la colección";
    try {
      await moverEntreColecciones(desde, hacia, docs);
      // Salen de la colección abierta: ni elegidos ni en la vista previa.
      this.elegidos = this.elegidos.filter((e) => !docs.includes(e));
      if (this.seleccion && docs.includes(this.seleccion) && this.abierta?.id === desde) this.seleccion = null;
      await this.recargarColecciones();
      this.version++;
      const a = this.colecciones.find((c) => c.id === hacia)?.name ?? "la colección";
      this.avisar(
        docs.length === 1
          ? `«${etiqueta}» movido de «${de}» a «${a}». En su carpeta no cambia nada.`
          : `${docs.length} documentos movidos de «${de}» a «${a}». En sus carpetas no cambia nada.`,
      );
    } catch (e) {
      this.avisar(`No se pudo mover — ${e}`);
    }
  }

  abrirBuscador(semilla = ""): void {
    if (!this.presentacionLista) return;
    this.semilla = semilla;
    this.pasajeAbierto = null;
    // Desde una colección abierta, se busca dentro de ella (ADR-0031); la ✕ del
    // buscador lo quita.
    this.buscarEn = this.vista === "coleccion" ? (this.abierta?.id ?? null) : null;
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
