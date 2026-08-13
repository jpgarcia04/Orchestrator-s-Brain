/**
 * El mapa. **Es la pantalla** (ADR-0013).
 *
 * Dos escalas del mismo grafo: el cielo —treinta regiones con nombre— y la
 * lente —una entidad y su vecindario—.
 *
 * ## La ley, que aquí se paga en cada decisión
 *
 * `EXPERIENCE.md` §1: **la luz es información, no decoración.** Ni un brillo
 * elegido a ojo. Todo lo que se ve codifica un dato que llega calculado desde
 * el núcleo (ADR-0012):
 *
 * | lo que se ve | lo que significa |
 * |---|---|
 * | **tamaño** | cuántos archivos hay dentro |
 * | **temperatura** | cuándo fue lo último que tocaste ahí |
 * | **luminosidad** | relevancia respecto a lo que buscas |
 * | **latido** | recencia — lo vivo respira, lo antiguo está quieto |
 * | **polvo** | un punto por archivo. La densidad *es* la cantidad |
 * | **trazo** | sólido = hecho · punteado = inferencia |
 * | **opacidad de arista** | confianza de esa relación concreta |
 *
 * La temperatura es la que cumple la frase del README —*«las zonas brillantes
 * son donde tu vida está activa ahora mismo»*— y por eso `recency` viaja dentro
 * de cada región y de cada nodo en vez de inventarse aquí.
 *
 * ## El reposo sigue siendo un compromiso, no una optimización
 *
 * §2: *«una galaxia WebGL a 60 fps constantes calienta el portátil y se come la
 * batería»*. El bucle corre **sólo** con la ventana enfocada y mientras haya
 * habido actividad en los últimos 25 s; después se para solo y vuelve al primer
 * movimiento. Con `prefers-reduced-motion` no arranca nunca y todo se dibuja
 * quieto: la aplicación sigue siendo completamente utilizable (§3).
 */

import {
  Application,
  Container,
  Graphics,
  Sprite,
  Text,
  Texture,
  TextStyle,
} from "pixi.js";
import type { Arista, Cielo, Lente, Region } from "./nucleo";

const FONDO = 0x05070c;

/** Frío: dormido. Cálido: tocado hace nada. §1 — el mapa te enseña dónde está
 *  viva tu vida ahora mismo, y eso es un dato, no una paleta. */
const FRIO = [0x5c, 0x7d, 0xb0] as const;
const CALIDO = [0xff, 0xd2, 0x9b] as const;
const VAULT = [0xd0, 0x6e, 0x6e] as const;

/** Iteraciones del layout. Fijas = deterministas. */
const ITERACIONES = 220;
/** Sin actividad, el bucle se para. §2. */
const REPOSO_MS = 25_000;
/** Techo de puntos de polvo. El prototipo aguanta 4.000 nodos a 3,2 ms. */
const POLVO_MAX = 3000;

const menosMovimiento = () =>
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

// ────────────────────────────────────────────────────────────────── texturas

/**
 * Un disco con caída suave, generado una vez y reutilizado por todo.
 *
 * Es lo que sustituye a los tres círculos concéntricos de la versión anterior,
 * y no es sólo más bonito: **es más barato**. Un `Sprite` con textura
 * compartida se agrupa con los demás en una sola llamada de dibujo, mientras
 * que cada `Graphics` construye su propia geometría. La estrella se ve como una
 * estrella y encima baja el coste.
 */
function discoSuave(tam: number, dureza: number): Texture {
  const c = document.createElement("canvas");
  c.width = c.height = tam;
  const g = c.getContext("2d")!;
  const r = tam / 2;
  const grad = g.createRadialGradient(r, r, 0, r, r, r);
  grad.addColorStop(0, "rgba(255,255,255,1)");
  grad.addColorStop(dureza, "rgba(255,255,255,0.85)");
  grad.addColorStop(0.55, "rgba(255,255,255,0.18)");
  grad.addColorStop(1, "rgba(255,255,255,0)");
  g.fillStyle = grad;
  g.fillRect(0, 0, tam, tam);
  return Texture.from(c);
}

/** Oscurece los bordes. Es lo único de este archivo que no codifica nada: no
 *  afirma, sólo deja de competir con el centro. */
function vineta(tam: number): Texture {
  const c = document.createElement("canvas");
  c.width = c.height = tam;
  const g = c.getContext("2d")!;
  const r = tam / 2;
  const grad = g.createRadialGradient(r, r, r * 0.28, r, r, r);
  grad.addColorStop(0, "rgba(0,0,0,0)");
  grad.addColorStop(1, "rgba(0,0,0,0.72)");
  g.fillStyle = grad;
  g.fillRect(0, 0, tam, tam);
  return Texture.from(c);
}

// ─────────────────────────────────────────────────────────────────── colores

function mezclar(a: readonly number[], b: readonly number[], t: number): number {
  const k = Math.max(0, Math.min(1, t));
  const r = Math.round(a[0]! + (b[0]! - a[0]!) * k);
  const g = Math.round(a[1]! + (b[1]! - a[1]!) * k);
  const z = Math.round(a[2]! + (b[2]! - a[2]!) * k);
  return (r << 16) | (g << 8) | z;
}

/**
 * Recencia → temperatura, con una curva y no una recta.
 *
 * El tiempo no se percibe lineal: entre *ayer* y *anteayer* hay una diferencia
 * enorme y entre *hace tres años* y *hace cuatro* casi ninguna. Una
 * interpolación recta sobre el rango del corpus dejaría todo del mismo color
 * salvo lo de la última semana. La potencia comprime lo viejo y expande lo
 * reciente, que es como se recuerda.
 */
function calor(recency: number, oldest: number, newest: number): number {
  if (recency <= 0 || newest <= oldest) return 0;
  const t = (recency - oldest) / (newest - oldest);
  return Math.pow(Math.max(0, Math.min(1, t)), 2.6);
}

// ─────────────────────────────────────────────────────────────────── layout

interface Cuerpo {
  id: string;
  seed: [number, number];
  masa: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
}

/**
 * Coloca. **Sin una sola llamada a `Math.random`.**
 *
 * El núcleo garantiza el mismo conjunto con las mismas posiciones de partida;
 * esta función garantiza que desde ahí se converge siempre al mismo sitio. El
 * jitter aleatorio que trae de serie casi todo simulador de fuerzas rompería
 * las dos garantías, y con ellas el capítulo 6 de `EXPERIENCE.md`, que se apoya
 * en que un nodo esté donde lo dejaste.
 */
function disponer(
  cuerpos: Cuerpo[],
  enlaces: ReadonlyArray<readonly [number, number]>,
  ancho: number,
  alto: number,
): void {
  const n = cuerpos.length;
  if (n === 0) return;
  const radio = Math.min(ancho, alto) * 0.38;
  for (const c of cuerpos) {
    c.x = c.seed[0] * radio;
    c.y = c.seed[1] * radio;
    c.vx = 0;
    c.vy = 0;
  }
  const reposo = radio / Math.sqrt(n + 1);

  for (let it = 0; it < ITERACIONES; it++) {
    const t = 1 - it / ITERACIONES;
    for (let i = 0; i < n; i++) {
      const a = cuerpos[i]!;
      for (let j = i + 1; j < n; j++) {
        const b = cuerpos[j]!;
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        let d2 = dx * dx + dy * dy;
        if (d2 < 1e-6) {
          dx = (i - j) * 0.01;
          dy = 0.01;
          d2 = dx * dx + dy * dy;
        }
        const f = ((reposo * reposo) / d2) * Math.sqrt(a.masa * b.masa);
        const d = Math.sqrt(d2);
        a.vx += (dx / d) * f;
        a.vy += (dy / d) * f;
        b.vx -= (dx / d) * f;
        b.vy -= (dy / d) * f;
      }
    }
    for (const [i, j] of enlaces) {
      const a = cuerpos[i]!;
      const b = cuerpos[j]!;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const d = Math.hypot(dx, dy) || 1e-3;
      const f = (d - reposo) * 0.06;
      a.vx += (dx / d) * f;
      a.vy += (dy / d) * f;
      b.vx -= (dx / d) * f;
      b.vy -= (dy / d) * f;
    }
    for (const c of cuerpos) {
      const k = 0.02 / Math.sqrt(c.masa);
      c.vx -= c.x * k;
      c.vy -= c.y * k;
      c.x += c.vx * t;
      c.y += c.vy * t;
      c.vx *= 0.82;
      c.vy *= 0.82;
    }
  }
}

/**
 * Estira lo colocado hasta llenar la ventana, conservando la forma.
 *
 * La simulación reparte sobre un disco de radio `min(ancho, alto)`, así que en
 * una pantalla apaisada el mapa salía como una bola en el centro con la mitad
 * del espacio vacío a los lados. Aquí no se recoloca nada —la topología ya está
 * decidida y es determinista—: sólo se escala y se centra lo que hay.
 *
 * El tope de 2,6× existe porque los radios de las estrellas **no** se escalan
 * —codifican cuántos archivos hay dentro y eso no depende del tamaño de la
 * ventana—, y estirar sin límite dejaría puntitos perdidos en un vacío.
 */
function encuadrar(cuerpos: Cuerpo[], ancho: number, alto: number): void {
  if (cuerpos.length < 2) return;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const c of cuerpos) {
    x0 = Math.min(x0, c.x);
    x1 = Math.max(x1, c.x);
    y0 = Math.min(y0, c.y);
    y1 = Math.max(y1, c.y);
  }
  const anchoActual = Math.max(1, x1 - x0);
  const altoActual = Math.max(1, y1 - y0);
  const k = Math.min(2.6, (ancho * 0.8) / anchoActual, (alto * 0.72) / altoActual);
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  for (const c of cuerpos) {
    c.x = (c.x - cx) * k;
    c.y = (c.y - cy) * k;
  }
}

/** Hash estable → 0..1. El mismo `splitmix64` que usa el núcleo para las
 *  posiciones de partida: hace falta que el polvo caiga siempre igual. */
function azarEstable(clave: string): number {
  let h = 0xcbf29ce484222325n;
  for (let i = 0; i < clave.length; i++) {
    h ^= BigInt(clave.charCodeAt(i) & 0xff);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  h ^= h >> 30n;
  h = (h * 0xbf58476d1ce4e5b9n) & 0xffffffffffffffffn;
  h ^= h >> 27n;
  h = (h * 0x94d049bb133111ebn) & 0xffffffffffffffffn;
  h ^= h >> 31n;
  return Number(h & 0xffffffn) / 0xffffff;
}

/**
 * El nombre más corto que siga siendo inequívoco **en este cielo**.
 *
 * El núcleo etiqueta las carpetas con dos segmentos porque cinco carpetas
 * llamadas `src` son cinco nodos indistinguibles. Pero cuando el padre es el
 * mismo para casi todas, esos dos segmentos repiten el nombre del repositorio
 * doce veces y no desambiguan nada: sólo tapan el mapa.
 *
 * La regla correcta no es fija, depende de lo que haya delante: se usa la hoja,
 * y sólo se añade el padre a las que chocan entre sí.
 */
function nombresCortos(regiones: ReadonlyArray<{ id: string; path: string; label: string }>) {
  const hoja = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const veces = new Map<string, number>();
  for (const r of regiones) {
    const h = hoja(r.path);
    veces.set(h, (veces.get(h) ?? 0) + 1);
  }
  const out = new Map<string, string>();
  for (const r of regiones) {
    const h = hoja(r.path);
    out.set(r.id, (veces.get(h) ?? 0) > 1 ? r.label : h);
  }
  return out;
}

const radioRegion = (peso: number) => Math.min(44, 7 + Math.sqrt(peso) * 3.1);
const radioNodo = (grado: number) => Math.min(15, 2.6 + Math.sqrt(grado) * 1.4);
const luz = (distancia: number) => Math.max(0.16, 1 - distancia * 0.42);
const solida = (e: Arista) => e.method === "deterministic" || e.method === "user";

/** Un cuerpo luminoso con su latido. */
interface Astro {
  halo: Sprite;
  nucleo: Sprite;
  base: number;
  /** Amplitud del latido ∝ recencia. Lo antiguo está quieto. */
  amplitud: number;
  fase: number;
  periodo: number;
}

export class Mapa {
  private app = new Application();
  private lienzo: HTMLCanvasElement;

  private fondo = new Container();
  private capaPolvo = new Container();
  private capaAristas = new Container();
  private capaHalos = new Container();
  private capaNucleos = new Container();
  private capaTexto = new Container();
  private mundo = new Container();

  private texDisco!: Texture;
  private texPunto!: Texture;
  private texVineta!: Texture;
  private spVineta!: Sprite;

  private cieloActual: Cielo | null = null;
  private lenteActual: Lente | null = null;
  private pos = new Map<string, Cuerpo>();
  private encendidas = new Set<string>();
  private astros: Astro[] = [];

  private corriendo = false;
  private ultimaActividad = performance.now();
  private inicio = performance.now();
  /** Asentamiento (§3 #1): 0 → 1 al cambiar de escala. */
  private asiento = 1;
  private paralaje = { x: 0, y: 0, tx: 0, ty: 0 };

  constructor(lienzo: HTMLCanvasElement) {
    this.lienzo = lienzo;
  }

  async init(): Promise<void> {
    await this.app.init({
      canvas: this.lienzo,
      background: FONDO,
      antialias: true,
      autoStart: false,
      autoDensity: true,
      resolution: window.devicePixelRatio || 1,
    });
    // **Sin `resizeTo: window`.** No aplica en el arranque: el renderizador se
    // quedaba en los 800×600 por defecto y todo el mapa se dibujaba apretado en
    // la esquina superior izquierda de la ventana, con un borde recto visible
    // donde acababa el lienzo. Se redimensiona a mano, que además deja claro
    // cuándo ocurre.
    this.ajustar();
    this.app.ticker.stop();
    this.app.ticker.add(this.bucle);

    this.texDisco = discoSuave(256, 0.14);
    this.texPunto = discoSuave(64, 0.05);
    this.texVineta = vineta(512);

    this.spVineta = new Sprite(this.texVineta);
    this.spVineta.anchor.set(0.5);
    this.fondo.addChild(this.spVineta);

    // El orden es la profundidad: polvo detrás, halos, aristas entre medias,
    // núcleos y texto delante.
    this.mundo.addChild(
      this.capaPolvo,
      this.capaHalos,
      this.capaAristas,
      this.capaNucleos,
      this.capaTexto,
    );
    this.app.stage.addChild(this.mundo, this.fondo);

    window.addEventListener("resize", () => {
      this.ajustar();
      this.recolocar();
      this.dibujar();
    });
    window.addEventListener("blur", () => this.parar());
    window.addEventListener("focus", () => this.despertar());
    window.addEventListener("pointermove", (e) => this.raton(e));
    window.addEventListener("keydown", () => this.despertar());
  }

  // ───────────────────────────────────────────────────────────── qué dibujar

  cielo(c: Cielo): void {
    this.cieloActual = c;
    this.lenteActual = null;
    this.encendidas.clear();
    this.asentar();
  }

  mostrar(l: Lente): void {
    this.lenteActual = l;
    this.cieloActual = null;
    this.asentar();
  }

  /** Enciende las regiones donde caen los resultados. **No recoloca nada.** */
  encender(ids: Iterable<string>): void {
    this.encendidas = new Set(ids);
    this.construir();
    this.dibujar();
  }

  /**
   * Congela la galaxia: un fotograma, el bucle parado y el lienzo desenfocado.
   *
   * ADR-0014 §3. Detrás del explorador hay **una fotografía, no una escena
   * viva**: se ve igual y respeta el compromiso de modo reposo de §2 — una
   * galaxia animada bajo una ventana durante media hora es batería gastada en
   * algo que nadie mira.
   */
  congelar(): void {
    this.asiento = 1;
    this.dibujar();
    this.parar();
    this.lienzo.classList.add("congelado");
  }

  descongelar(): void {
    this.lienzo.classList.remove("congelado");
    this.despertar();
    if (!this.corriendo) this.dibujar();
  }

  regionEn(x: number, y: number): Region | null {
    if (!this.cieloActual) return null;
    // `mundo.position` **ya lleva el paralaje dentro** — lo mete `latir` en cada
    // fotograma. Restarlo otra vez aquí lo contaba dos veces, y como el paralaje
    // persigue al ratón, el error era máximo justo donde estaba el cursor: hasta
    // 18 px por eje en los bordes, contra estrellas de 7 px de radio. Se picaba
    // una y se abría la de al lado.
    const lx = x - this.mundo.x;
    const ly = y - this.mundo.y;
    let mejor: Region | null = null;
    let mejorD = Infinity;
    for (const r of this.cieloActual.regions) {
      const p = this.pos.get(r.id);
      if (!p) continue;
      const d = Math.hypot(lx - p.x, ly - p.y);
      if (d < radioRegion(r.weight) + 8 && d < mejorD) {
        mejor = r;
        mejorD = d;
      }
    }
    return mejor;
  }

  // ─────────────────────────────────────────────────────────────── el bucle

  private asentar(): void {
    this.recolocar();
    // §3 #1: los nodos nunca *aparecen*, llegan y se asientan. ~400 ms.
    //
    // Pero sólo si se puede animar. Si no —movimiento reducido, o la ventana
    // abierta sin foco— el asentamiento se da por terminado en vez de quedarse
    // a medias: dejarlo en cero pintaba el mapa al 25 % de opacidad y ahí se
    // quedaba, porque el bucle que lo subía nunca llegaba a arrancar. La
    // disciplina de §3 dice que sin animación la aplicación sigue siendo
    // completamente utilizable, no que se quede traslúcida.
    this.asiento = menosMovimiento() || !document.hasFocus() ? 1 : 0;
    this.despertar();
    // Un fotograma **siempre**, corra o no el bucle.
    if (!this.corriendo) this.dibujar();
  }

  private despertar(): void {
    this.ultimaActividad = performance.now();
    if (this.corriendo || menosMovimiento() || !document.hasFocus()) return;
    this.corriendo = true;
    this.app.ticker.start();
  }

  private parar(): void {
    if (!this.corriendo) return;
    this.corriendo = false;
    this.app.ticker.stop();
    // Un último fotograma ya asentado, para que no se congele a media
    // respiración ni a media entrada.
    this.asiento = 1;
    this.dibujar();
  }

  private raton(e: PointerEvent): void {
    const [w, h] = this.medidas();
    // Profundidad simulada (§7.5): las capas se desplazan distinto. Es el 3D
    // que este grafo admite — el otro, el de verdad, hace la topología ilegible
    // por oclusión y ambigüedad de profundidad.
    this.paralaje.tx = ((e.clientX - w / 2) / w) * -18;
    this.paralaje.ty = ((e.clientY - h / 2) / h) * -18;
    this.despertar();
    if (!this.corriendo) this.dibujar();
  }

  private bucle = (): void => {
    const ahora = performance.now();
    if (!document.hasFocus() || ahora - this.ultimaActividad > REPOSO_MS) {
      this.parar();
      return;
    }
    if (this.asiento < 1) this.asiento = Math.min(1, this.asiento + 0.055);
    this.paralaje.x += (this.paralaje.tx - this.paralaje.x) * 0.08;
    this.paralaje.y += (this.paralaje.ty - this.paralaje.y) * 0.08;
    this.latir(ahora);
    this.app.render();
  };

  /**
   * §3 #4. *«Amplitud mínima, periodo largo, desincronizado por nodo, amplitud
   * ∝ recencia»* — y sobre todo: *«si conscientemente ves los nodos latir, está
   * demasiado fuerte. Debe notarse al quitar la vista, no al mirarlo.»*
   *
   * De ahí que el máximo sea un 5 % de escala. Es lo que hace real la palabra
   * «respirar» de la visión original, y con función: lo que respira es lo que
   * tocaste hace poco.
   */
  private latir(ahora: number): void {
    const t = (ahora - this.inicio) / 1000;
    const entrada = this.asiento;
    for (const a of this.astros) {
      const pulso = Math.sin((t / a.periodo) * Math.PI * 2 + a.fase);
      const k = (1 + a.amplitud * pulso) * (0.92 + 0.08 * entrada);
      a.halo.scale.set(a.base * 3.1 * k);
      a.nucleo.scale.set(a.base * k);
      a.halo.alpha = a.halo.label ? Number(a.halo.label) * (0.85 + 0.15 * pulso) * entrada : 0;
      a.nucleo.alpha = a.nucleo.label ? Number(a.nucleo.label) * entrada : 0;
    }
    this.mundo.alpha = 0.25 + 0.75 * entrada;
    const [w, h] = this.medidas();
    this.mundo.position.set(w / 2 + this.paralaje.x, h / 2 + this.paralaje.y);
    this.capaPolvo.position.set(this.paralaje.x * 0.9, this.paralaje.y * 0.9);
    // El polvo sí flota —es ambiente y no afirma nada—, pero **el nombre no se
    // despega nunca de su estrella**. Llevaba un paralaje contrario (−0,35) que,
    // sumado al de `mundo`, dejaba la etiqueta a ~6 px de su propio nodo y
    // moviéndose con el ratón. Un nombre que flota entre dos estrellas no dice
    // cuál de las dos nombra, y §1 no admite un movimiento que destruya la
    // información que el texto es.
    this.capaTexto.position.set(0, 0);
  }

  private ajustar(): void {
    this.app.renderer.resize(window.innerWidth, window.innerHeight);
  }

  /**
   * El tamaño del lienzo **en píxeles lógicos**.
   *
   * `app.screen` y no `renderer.width / resolution`: en Pixi v8 el ancho del
   * renderizador ya viene en unidades lógicas, así que dividirlo otra vez por
   * la densidad lo encogía. En una pantalla sin escalado no se notaba —la
   * densidad es 1 y dividir no hace nada—; en una escalada al 300 % el mapa se
   * dibujaba en un tercio de la ventana, apretado en la esquina y con el borde
   * del lienzo a la vista. Es el defecto que sólo aparece en la máquina de
   * alguien.
   */
  private medidas(): [number, number] {
    return [this.app.screen.width, this.app.screen.height];
  }

  private dibujar(): void {
    this.latir(performance.now());
    this.app.render();
  }

  // ──────────────────────────────────────────────────────────── construcción

  private recolocar(): void {
    const [w, h] = this.medidas();
    this.mundo.position.set(w / 2, h / 2);
    this.spVineta.position.set(w / 2, h / 2);
    this.spVineta.scale.set((Math.max(w, h) * 1.6) / 512);
    this.pos.clear();

    const fuente = this.cieloActual
      ? this.cieloActual.regions.map((r) => ({
          id: r.id,
          seed: r.seed,
          masa: Math.max(1, r.weight),
        }))
      : (this.lenteActual?.nodes.map((n) => ({ id: n.id, seed: n.seed, masa: 1 })) ?? []);

    const cuerpos: Cuerpo[] = fuente.map((f) => ({ ...f, x: 0, y: 0, vx: 0, vy: 0 }));
    const indice = new Map(cuerpos.map((c, i) => [c.id, i]));
    const crudas = this.cieloActual
      ? this.cieloActual.bridges.map((b) => [b.from, b.to] as const)
      : (this.lenteActual?.edges.map((e) => [e.from, e.to] as const) ?? []);
    const enlaces = crudas
      .map(([a, b]) => [indice.get(a), indice.get(b)] as const)
      .filter((p): p is readonly [number, number] => p[0] !== undefined && p[1] !== undefined);

    disponer(cuerpos, enlaces, w, h);
    encuadrar(cuerpos, w, h);
    for (const c of cuerpos) this.pos.set(c.id, c);
    this.construir();
  }

  private limpiar(): void {
    for (const capa of [
      this.capaPolvo,
      this.capaAristas,
      this.capaHalos,
      this.capaNucleos,
      this.capaTexto,
    ]) {
      capa.removeChildren().forEach((c) => c.destroy());
    }
    this.astros = [];
  }

  /** Un astro: halo difuso detrás, núcleo nítido delante. El alfa de reposo
   *  viaja en `label` porque el latido lo modula y hace falta el original. */
  private astro(x: number, y: number, radio: number, color: number, brillo: number, recencia: number, clave: string): void {
    const halo = new Sprite(this.texDisco);
    halo.anchor.set(0.5);
    halo.position.set(x, y);
    halo.tint = color;
    halo.blendMode = "add";
    halo.label = String(brillo * 0.34);
    this.capaHalos.addChild(halo);

    const nucleo = new Sprite(this.texDisco);
    nucleo.anchor.set(0.5);
    nucleo.position.set(x, y);
    nucleo.tint = color;
    nucleo.blendMode = "add";
    nucleo.label = String(Math.min(1, brillo));
    this.capaNucleos.addChild(nucleo);

    this.astros.push({
      halo,
      nucleo,
      base: (radio * 2.2) / 256,
      // Lo antiguo está quieto. Lo de esta semana respira.
      amplitud: menosMovimiento() ? 0 : 0.012 + recencia * 0.038,
      fase: azarEstable(clave) * Math.PI * 2,
      periodo: 3.2 + azarEstable(`${clave}:p`) * 2.0,
    });
  }

  private construir(): void {
    this.limpiar();
    if (this.cieloActual) this.construirCielo(this.cieloActual);
    else if (this.lenteActual) this.construirLente(this.lenteActual);
  }

  private construirCielo(c: Cielo): void {
    const hayFoco = this.encendidas.size > 0;
    const presupuesto = Math.min(POLVO_MAX, c.covered);
    const escala = c.covered > 0 ? presupuesto / c.covered : 0;

    // ── puentes ── curvas, no rectas: una recta se lee como diagrama y una
    // curva como órbita, y esto es un cielo.
    const trazos = new Graphics();
    for (const b of c.bridges) {
      const a = this.pos.get(b.from);
      const z = this.pos.get(b.to);
      if (!a || !z) continue;
      const vivo = !hayFoco || this.encendidas.has(b.from) || this.encendidas.has(b.to);
      const mx = (a.x + z.x) / 2;
      const my = (a.y + z.y) / 2;
      // Comba perpendicular, hacia fuera del centro.
      const nx = -(z.y - a.y);
      const ny = z.x - a.x;
      const largo = Math.hypot(nx, ny) || 1;
      const comba = Math.min(60, largo * 0.14);
      trazos
        .moveTo(a.x, a.y)
        .quadraticCurveTo(mx + (nx / largo) * comba, my + (ny / largo) * comba, z.x, z.y);
      trazos.stroke({
        color: 0x86a8d8,
        alpha: (vivo ? 0.14 : 0.025) + Math.min(0.18, b.weight * 0.018),
        width: Math.min(2.6, 0.5 + b.weight * 0.22),
      });
    }
    trazos.blendMode = "add";
    this.capaAristas.addChild(trazos);

    const estilo = new TextStyle({
      fill: 0xcfdcec,
      fontSize: 11,
      fontFamily: "Segoe UI, system-ui, sans-serif",
      letterSpacing: 0.7,
      dropShadow: { color: 0x000000, alpha: 0.9, blur: 6, distance: 0 },
    });
    const cortos = nombresCortos(c.regions);
    /** Cajas de las etiquetas ya colocadas, para no pisarlas. */
    const ocupado: Array<[number, number, number, number]> = [];

    for (const r of c.regions) {
      const p = this.pos.get(r.id);
      if (!p) continue;
      const rad = radioRegion(r.weight);
      const viva = !hayFoco || this.encendidas.has(r.id);
      const brillo = viva ? 1 : 0.14;
      const temp = calor(r.recency, c.oldest, c.newest);
      const color =
        r.sensitivity === "vault" ? mezclar(VAULT, VAULT, 0) : mezclar(FRIO, CALIDO, temp);

      // ── polvo: un punto por archivo ──
      // No es textura de relleno: la densidad *es* la cantidad, igual que el
      // tamaño. Un punto de mapa de puntos, que es cartografía de toda la vida.
      const cuantos = Math.max(1, Math.round(r.weight * escala));
      for (let i = 0; i < cuantos; i++) {
        const a = azarEstable(`${r.id}:${i}:a`) * Math.PI * 2;
        const d = rad * (1.25 + Math.sqrt(azarEstable(`${r.id}:${i}:d`)) * 2.6);
        const s = new Sprite(this.texPunto);
        s.anchor.set(0.5);
        s.position.set(p.x + Math.cos(a) * d, p.y + Math.sin(a) * d);
        s.scale.set((1.1 + azarEstable(`${r.id}:${i}:s`) * 1.5) / 64 * 6);
        s.tint = color;
        s.alpha = (0.1 + azarEstable(`${r.id}:${i}:o`) * 0.28) * brillo;
        s.blendMode = "add";
        this.capaPolvo.addChild(s);
      }

      this.astro(p.x, p.y, rad, color, brillo, temp, r.id);

      // Las regiones vienen ordenadas de mayor a menor, así que las grandes
      // reclaman su sitio primero y las pequeñas ceden. Una etiqueta encima de
      // otra no es que se lea mal: es que **no se lee ninguna de las dos**, y
      // dos nombres ilegibles informan menos que un nombre y un punto sin él.
      const t = new Text({ text: cortos.get(r.id) ?? r.label, style: estilo });
      t.anchor.set(0.5, 0);
      t.position.set(p.x, p.y + rad + 7);
      const caja: [number, number, number, number] = [
        p.x - t.width / 2 - 4,
        t.y - 2,
        p.x + t.width / 2 + 4,
        t.y + t.height + 2,
      ];
      const choca = ocupado.some(
        (o) => caja[0] < o[2] && caja[2] > o[0] && caja[1] < o[3] && caja[3] > o[1],
      );
      if (choca) {
        t.destroy(true);
        continue;
      }
      ocupado.push(caja);
      t.alpha = viva ? 0.88 : 0.16;
      this.capaTexto.addChild(t);
    }
  }

  private construirLente(l: Lente): void {
    const tiempos = l.nodes.map((n) => n.recency).filter((t) => t > 0);
    const oldest = tiempos.length ? Math.min(...tiempos) : 0;
    const newest = tiempos.length ? Math.max(...tiempos) : 0;

    const trazos = new Graphics();
    for (const e of l.edges) {
      const a = this.pos.get(e.from);
      const b = this.pos.get(e.to);
      if (!a || !b) continue;
      const alfa = 0.1 + e.confidence * 0.26;
      if (solida(e)) {
        trazos.moveTo(a.x, a.y).lineTo(b.x, b.y);
        trazos.stroke({ color: 0x8fb2dd, alpha: alfa, width: 1 });
      } else {
        // Punteada a mano: Pixi no trae trazo discontinuo, y la distinción
        // hecho/inferencia de §1 no se negocia por una limitación de la
        // librería de dibujo.
        punteada(trazos, a.x, a.y, b.x, b.y);
        trazos.stroke({ color: 0x8fb2dd, alpha: alfa * 0.85, width: 1 });
      }
    }
    trazos.blendMode = "add";
    this.capaAristas.addChild(trazos);

    for (const n of l.nodes) {
      const p = this.pos.get(n.id);
      if (!p) continue;
      const temp = calor(n.recency, oldest, newest);
      const color = n.sensitivity === "vault" ? mezclar(VAULT, VAULT, 0) : mezclar(FRIO, CALIDO, temp);
      this.astro(p.x, p.y, radioNodo(n.degree), color, luz(n.distance), temp, n.id);
    }

    // Sólo el foco lleva etiqueta: trescientas serían trescientas razones para
    // no mirar el mapa.
    const estilo = new TextStyle({
      fill: 0xdfeaf8,
      fontSize: 11,
      fontFamily: "Segoe UI, system-ui, sans-serif",
      dropShadow: { color: 0x000000, alpha: 0.85, blur: 5, distance: 0 },
    });
    for (const n of l.nodes) {
      if (n.distance !== 0) continue;
      const p = this.pos.get(n.id);
      if (!p) continue;
      const t = new Text({ text: n.label, style: estilo });
      t.anchor.set(0.5, 0);
      t.position.set(p.x, p.y + radioNodo(n.degree) + 6);
      t.alpha = 0.9;
      this.capaTexto.addChild(t);
    }
  }
}

function punteada(g: Graphics, x1: number, y1: number, x2: number, y2: number): void {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const n = Math.max(1, Math.floor(Math.hypot(dx, dy) / 7));
  for (let i = 0; i < n; i += 2) {
    const t0 = i / n;
    const t1 = Math.min(1, (i + 1) / n);
    g.moveTo(x1 + dx * t0, y1 + dy * t0).lineTo(x1 + dx * t1, y1 + dy * t1);
  }
}
