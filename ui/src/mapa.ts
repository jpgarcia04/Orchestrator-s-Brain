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
import type { Arista, Cielo, Lente, Nodo, Region } from "./nucleo";

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

/**
 * Los topes de la cámara. **`z = 1` es el encuadre completo**: en el prototipo
 * hacía falta un `zBase` porque el cielo se generaba en unidades propias, y
 * aquí `recolocar` ya estira el layout hasta llenar la ventana. La unidad ya
 * significa «cabe todo», así que volver a casa es `z = 1` y nada más.
 */
const Z_MIN = 0.65;
const Z_MAX = 14;

/**
 * Cuánto crece lo que se ve al acercarse. **No es una constante de estilo.**
 *
 * Si el tamaño de un nodo escalara con el zoom (`z^1`), acercarse convertiría
 * el cielo en una mancha y el tamaño pasaría a significar *distancia de
 * cámara*. §1 le tiene asignado otro significado —densidad de conexiones, o
 * cuántos archivos hay dentro— y ése no se negocia por comodidad de dibujo.
 * Amortiguado, acercarse revela **estructura** y el canal sigue diciendo lo
 * que dice. Es el `zVis` del prototipo, medido allí antes de escribirse aquí.
 */
const AMORTIGUA = 0.34;

/** Radio en pantalla por debajo del cual un nombre no se gana su sitio. */
const RADIO_ROTULO = 6.5;
/** Techo de nombres a la vez. Trescientos serían trescientas razones para no
 *  mirar el mapa; la escala decide cuáles, este número decide cuántos. */
const ROTULOS_MAX = 26;

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
 *
 * ## Convergía con treinta nodos y **divergía con trescientos**
 *
 * Lo destapó la cámara, que es la única razón por la que se toca: al acercarse
 * se veía que 233 de los 271 nodos de una lente real estaban **en el mismo
 * píxel**. Medido en aislamiento contra el corpus: las posiciones llegaban a
 * 1e+126, y `encuadrar` —que reescala a la ventana— aplastaba todo el rango
 * astronómico en un punto. Por eso «la selección de nodos está muy extraña» no
 * era una impresión: no había nodos que seleccionar, había un montón.
 *
 * La repulsión `reposo²/d²` no tiene tope, y con 271 cuerpos que arrancan en un
 * cuadrante hay pares muy juntos desde el primer paso: la velocidad se
 * realimenta y la amortiguación de 0,82 no puede con una explosión
 * superexponencial. Se arregla como lo arregla Fruchterman-Reingold —**el paso
 * tiene un máximo y el máximo se enfría**— más un suelo de distancia para que
 * un par pegado no aporte una fuerza infinita.
 *
 * | lente real | antes | ahora |
 * |---|---|---|
 * | 271 nodos | 32 posiciones distintas, separación 0 | **271 distintas, separación 10,9 px** |
 * | 38 · 31 · 35 nodos | ya convergían | igual o algo mejor |
 * | cielo, 30 regiones | convergía | converge |
 *
 * Y de paso baja de 82 ms a 52 ms, porque dejan de hacerse cuentas con números
 * de ciento veintiséis cifras. **No es el arreglo del O(n²)**: eso sigue
 * pendiente y sigue siendo otro trabajo.
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
  /** Dos cuerpos más juntos que esto se repelen como si estuvieran aquí. */
  const d2min = (reposo * 0.35) ** 2;

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
        if (d2 < d2min) d2 = d2min;
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
      // El paso tiene un máximo y el máximo se enfría con `t`. Es lo único que
      // separa un layout que converge de uno que se va a 1e+126: sin tope, una
      // velocidad grande produce una posición grande, que produce una velocidad
      // mayor. Con tope, el sistema no puede escaparse por mucho que empuje.
      const v = Math.hypot(c.vx, c.vy);
      const maximo = reposo * 2 * t;
      const paso = v * t > maximo ? maximo / v : t;
      c.x += c.vx * paso;
      c.y += c.vy * paso;
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

/**
 * Un solo estilo para todos los nombres, del cielo y de la lente.
 *
 * Antes había dos, y la diferencia entre ellos —un tono y un desenfoque de
 * sombra— no codificaba nada. §1 no prohíbe sólo el brillo decorativo: dos
 * tipografías distintas para la misma clase de cosa son una distinción que el
 * ojo intenta leer y que no dice nada.
 */
const ESTILO_ROTULO = new TextStyle({
  fill: 0xd7e3f4,
  fontSize: 11,
  fontFamily: "Segoe UI, system-ui, sans-serif",
  letterSpacing: 0.7,
  dropShadow: { color: 0x000000, alpha: 0.9, blur: 6, distance: 0 },
});

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

/**
 * Algo que se puede picar, nombrar y realzar: una región del cielo o un nodo
 * de la lente. Las dos escalas se dibujan distinto y **se manejan igual**, así
 * que el picado, los nombres y el realce se escriben una vez sobre esto.
 */
interface Picable {
  id: string;
  /** Posición en el mundo, la que produce el layout. */
  x: number;
  y: number;
  /** Radio a zoom 1. En pantalla es `radio · z^0,34`. */
  radio: number;
  texto: string;
  /** Quién se queda el sitio cuando dos nombres chocan. */
  peso: number;
  /** Encendido (§1: luminosidad = relevancia). Atenúa también su nombre. */
  vivo: boolean;
  /** El dato del núcleo, para devolverlo entero sin volver a buscarlo. */
  fuente: Diana;
}

/** Lo que hay debajo del cursor cuando se pulsa. */
export type Diana = { tipo: "region"; region: Region } | { tipo: "nodo"; nodo: Nodo };
export type Picado = Diana | null;

/** Un nombre en pantalla, con su opacidad de destino para el fundido. */
interface Rotulo {
  txt: Text;
  objetivo: number;
}

export class Mapa {
  private app = new Application();
  private lienzo: HTMLCanvasElement;

  private fondo = new Container();
  private capaPolvo = new Container();
  private capaAristas = new Container();
  private capaHalos = new Container();
  private capaNucleos = new Container();
  private mundo = new Container();
  /** En coordenadas de **pantalla**, fuera de `mundo`: ver `rotular`. */
  private capaTexto = new Container();
  private capaRealce = new Container();

  private texDisco!: Texture;
  private texPunto!: Texture;
  private texVineta!: Texture;
  private spVineta!: Sprite;

  private cieloActual: Cielo | null = null;
  private lenteActual: Lente | null = null;
  private pos = new Map<string, Cuerpo>();
  private encendidas = new Set<string>();
  private astros: Astro[] = [];
  /** El polvo, con su escala de reposo: la amortiguación del zoom la modula. */
  private polvo: Array<{ sp: Sprite; base: number }> = [];

  private corriendo = false;
  private ultimaActividad = performance.now();
  private inicio = performance.now();
  /** Asentamiento (§3 #1): 0 → 1 al cambiar de escala. */
  private asiento = 1;
  private paralaje = { x: 0, y: 0, tx: 0, ty: 0 };

  // ── la cámara ──
  /** Dónde está. Es lo que se dibuja y **lo que se pica** (I10). */
  private cam = { x: 0, y: 0, z: 1 };
  /** A dónde va. La rueda mueve esto; el arrastre mueve los dos. */
  private destino = { x: 0, y: 0, z: 1 };
  private arrastre: { sx: number; sy: number; cx: number; cy: number; mov: number } | null = null;
  private congelado = false;
  /** Última composición aplicada a los rótulos y realces, para no rehacerlos
   *  sesenta veces por segundo cuando nada se ha movido. */
  private aplicada = { x: 0, y: 0, z: 0, px: 0, py: 0 };
  /** Zoom con el que se dibujaron aristas y polvo. Repintar en cada fotograma
   *  de un zoom sería tirar la batería por la ventana (§2). */
  private zPintado = 1;

  // ── qué vas a abrir ──
  private cuerpos: Picable[] = [];
  private encima: Picable | null = null;
  private elegido: string | null = null;
  private rotulos = new Map<string, Rotulo>();
  private realce = new Graphics();

  /** Lo que hay bajo el cursor al soltar. Lo pone `main`: el mapa sabe **qué**
   *  se ha picado, no qué hacer con ello. */
  onElegir: (p: Picado) => void = () => {};
  /** Avisa cuando la cámara se va del encuadre completo, y cuando vuelve. Es
   *  lo que hace visible la forma de volver: adivinar no es una salida. */
  onCamara: (movida: boolean) => void = () => {};

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

    // El orden es la profundidad: polvo detrás, halos, aristas entre medias y
    // núcleos delante.
    this.mundo.addChild(this.capaPolvo, this.capaHalos, this.capaAristas, this.capaNucleos);
    this.capaRealce.addChild(this.realce);
    // Realces y nombres van **por encima de la viñeta**, y es a propósito: la
    // viñeta es lo único de este archivo que no codifica nada, y un nombre sí.
    // Que lo que informa quede debajo de lo que sólo ambienta es exactamente el
    // orden que §1 prohíbe.
    this.app.stage.addChild(this.mundo, this.fondo, this.capaRealce, this.capaTexto);

    window.addEventListener("resize", () => {
      this.ajustar();
      // El layout se reescala a la ventana nueva, así que el mundo de debajo de
      // la cámara ya no es el mismo. Encuadrar de nuevo es lo predecible: dejar
      // la cámara donde estaba la dejaría apuntando a otro sitio del mapa.
      this.aEncuadre(true);
      this.recolocar();
      this.dibujar();
    });
    window.addEventListener("blur", () => this.parar());
    window.addEventListener("focus", () => this.despertar());
    window.addEventListener("pointermove", (e) => this.raton(e));
    window.addEventListener("keydown", () => this.despertar());

    this.lienzo.addEventListener("wheel", (e) => this.rueda(e), { passive: false });
    this.lienzo.addEventListener("pointerdown", (e) => this.agarrar(e));
    this.lienzo.addEventListener("pointerup", (e) => this.soltar(e));
    this.lienzo.addEventListener("pointercancel", () => {
      this.arrastre = null;
      this.lienzo.style.cursor = "grab";
    });
    this.lienzo.addEventListener("dblclick", (e) => this.acercarA(e));
    this.lienzo.style.cursor = "grab";
  }

  // ─────────────────────────────────────────────────────────────── la cámara

  /**
   * **La composición, y no hay otra.**
   *
   * Tres cosas quieren mover lo mismo: el layout, el paralaje y ahora la
   * cámara. Cuando cada una se aplicaba por su lado hubo que descubrir a mano
   * —y tarde— que `regionEn` restaba el paralaje dos veces y picaba la estrella
   * de al lado. Con una cámara encima eso deja de ser un fallo y pasa a ser una
   * familia de fallos, así que el orden se escribe **una vez aquí** y todo lo
   * demás —dibujo, picado, nombres, realces— sale de estas dos funciones:
   *
   * ```
   * pantalla = (mundo − cámara) · zoom + centro + paralaje
   * ```
   */
  private aPantalla(wx: number, wy: number): [number, number] {
    const [w, h] = this.medidas();
    return [
      (wx - this.cam.x) * this.cam.z + w / 2 + this.paralaje.x,
      (wy - this.cam.y) * this.cam.z + h / 2 + this.paralaje.y,
    ];
  }

  /** La inversa exacta de [`aPantalla`](Mapa#aPantalla). */
  private aMundo(sx: number, sy: number): [number, number] {
    const [w, h] = this.medidas();
    return [
      (sx - w / 2 - this.paralaje.x) / this.cam.z + this.cam.x,
      (sy - h / 2 - this.paralaje.y) / this.cam.z + this.cam.y,
    ];
  }

  /** El radio que ocupa algo en pantalla. Amortiguado: ver `AMORTIGUA`. */
  private radioVisual(radio: number): number {
    return radio * Math.pow(this.cam.z, AMORTIGUA);
  }

  /** Aplica la composición a los contenedores. Un solo sitio, otra vez. */
  private componer(): void {
    const [w, h] = this.medidas();
    const { x, y, z } = this.cam;
    this.mundo.scale.set(z);
    this.mundo.position.set(
      w / 2 + this.paralaje.x - x * z,
      h / 2 + this.paralaje.y - y * z,
    );
    // El polvo flota un poco más que las estrellas —es lo que da profundidad
    // sin 3D (§7.5)—, y ese extra se pide en píxeles de pantalla, así que hay
    // que dividirlo por el zoom para que no crezca al acercarse.
    this.capaPolvo.position.set((this.paralaje.x * 0.9) / z, (this.paralaje.y * 0.9) / z);
  }

  /** Vuelve al encuadre completo. `ya` salta sin interpolar. */
  private aEncuadre(ya = false): void {
    this.destino = { x: 0, y: 0, z: 1 };
    if (ya) this.cam = { ...this.destino };
  }

  /** ¿Está la cámara fuera del encuadre completo? */
  private get movida(): boolean {
    return Math.abs(this.destino.z - 1) > 0.01 || Math.hypot(this.destino.x, this.destino.y) > 1;
  }

  /**
   * Volver a ver todo. Hay tres formas de pedirlo —el botón, `Esc` y dos clics
   * en el vacío— y las tres son ésta.
   *
   * Devuelve si había algo que deshacer, porque `Esc` es una escalera: deshace
   * **un** escalón cada vez, y hace falta saber si éste era el que tocaba.
   */
  encuadre(): boolean {
    if (!this.movida) return false;
    this.aEncuadre();
    this.onCamara(false);
    this.mover();
    return true;
  }

  /**
   * Acercarse a un cuerpo. `irA` del prototipo: se le **pide un destino** a la
   * cámara, nunca se la teletransporta (§3, animación 2).
   */
  private irA(c: Picable): void {
    const [w, h] = this.medidas();
    this.destino = {
      x: c.x,
      y: c.y,
      z: Math.min(Z_MAX, (Math.min(w, h) * 0.34) / Math.max(c.radio, 5)),
    };
    this.onCamara(true);
    this.mover();
  }

  /**
   * Zoom **hacia el cursor**, no hacia el centro.
   *
   * No es un detalle de acabado: acercarse a una estrella y que se te escape
   * mientras te acercas es lo que hace que un mapa se sienta ajeno. El punto
   * del mundo que hay debajo del ratón se queda exactamente donde está.
   */
  private rueda(e: WheelEvent): void {
    if (this.congelado) return;
    e.preventDefault();
    const f = Math.exp(-e.deltaY * 0.0016);
    const nz = Math.min(Z_MAX, Math.max(Z_MIN, this.destino.z * f));
    const [w, h] = this.medidas();
    const [wx, wy] = this.aMundo(e.clientX, e.clientY);
    this.destino = {
      z: nz,
      x: wx - (e.clientX - w / 2 - this.paralaje.x) / nz,
      y: wy - (e.clientY - h / 2 - this.paralaje.y) / nz,
    };
    this.onCamara(this.movida);
    this.mover();
  }

  private agarrar(e: PointerEvent): void {
    if (this.congelado || e.button !== 0) return;
    this.arrastre = { sx: e.clientX, sy: e.clientY, cx: this.destino.x, cy: this.destino.y, mov: 0 };
    this.lienzo.setPointerCapture(e.pointerId);
    this.lienzo.style.cursor = "grabbing";
  }

  private soltar(e: PointerEvent): void {
    const era = this.arrastre;
    this.arrastre = null;
    this.lienzo.style.cursor = this.encima ? "pointer" : "grab";
    if (this.congelado || !era) return;
    // Cuatro píxeles: fue un arrastre, no un clic. Sin esto, mover el mapa
    // abriría el panel de la estrella que quedara debajo al soltar.
    if (era.mov > 4) return;
    const p = this.picar(e.clientX, e.clientY);
    this.onElegir(p);
  }

  /** Dos clics: acercarse a lo que hay debajo, o volver a verlo todo. */
  private acercarA(e: MouseEvent): void {
    if (this.congelado) return;
    const c = this.cuerpoEn(e.clientX, e.clientY);
    if (c) this.irA(c);
    else this.encuadre();
  }

  /** Pide un fotograma aunque el bucle esté parado: I10 otra vez. */
  private mover(): void {
    this.despertar();
    if (!this.corriendo) {
      this.cam = { ...this.destino };
      this.dibujar();
    }
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
    this.congelado = true;
    this.arrastre = null;
    this.señalar(null);
    this.asiento = 1;
    this.dibujar();
    this.parar();
    this.lienzo.classList.add("congelado");
  }

  descongelar(): void {
    this.congelado = false;
    this.lienzo.classList.remove("congelado");
    this.despertar();
    if (!this.corriendo) this.dibujar();
  }

  /**
   * Qué hay bajo el cursor. **Una función, no dos.**
   *
   * Antes había dos —una para el cielo y otra para la lente— y las dos
   * repetían a mano la conversión de pantalla a mundo. Eso ya costó un fallo
   * real: se restaba el paralaje dos veces, el error era máximo justo donde
   * estaba el cursor, y se picaba una estrella para abrir la de al lado. Con
   * una cámara encima, dos copias de la conversión son dos sitios donde volver
   * a equivocarse, así que aquí sólo se compara distancia y el `dónde` sale de
   * [`aPantalla`](Mapa#aPantalla).
   *
   * Se compara **en pantalla**: los 8 px de margen son de dedo, no de mundo, y
   * a zoom 6 un margen de mundo sería un margen de 48 px.
   */
  private cuerpoEn(sx: number, sy: number): Picable | null {
    let mejor: Picable | null = null;
    let mejorD = Infinity;
    for (const c of this.cuerpos) {
      const [x, y] = this.aPantalla(c.x, c.y);
      const d = Math.hypot(sx - x, sy - y);
      if (d < this.radioVisual(c.radio) + 8 && d < mejorD) {
        mejor = c;
        mejorD = d;
      }
    }
    return mejor;
  }

  private picar(sx: number, sy: number): Picado {
    return this.cuerpoEn(sx, sy)?.fuente ?? null;
  }

  /**
   * Marca lo que está debajo del puntero. **Es media pieza 2.**
   *
   * `cuerpoEn` existía desde M5b y sólo se llamaba al hacer clic: se picaba a
   * ciegas, sin cursor, sin realce y sin nombre. Saber qué vas a abrir antes de
   * abrirlo no es acabado, es la diferencia entre un mapa y una figura.
   */
  private señalar(c: Picable | null): void {
    // El cursor se pone **siempre**, aunque no cambie lo señalado. Al cambiar
    // de escala se olvida lo que había debajo, y la mano se quedaba puesta
    // sobre el vacío hasta que pasabas por otro nodo: un cursor que promete
    // algo que no está debajo es de la misma familia que picar a ciegas.
    if (!this.arrastre) this.lienzo.style.cursor = c ? "pointer" : "grab";
    if (c?.id === this.encima?.id) return;
    this.encima = c;
    this.superponer();
  }

  /** Cuál de los trescientos puntos es el que estás mirando. Lo dice `main`
   *  cuando abre el panel: sin esto, el mapa se calla justo después de que le
   *  hayas preguntado. */
  seleccionar(id: string | null): void {
    if (this.elegido === id) return;
    this.elegido = id;
    this.superponer();
    if (!this.corriendo) this.dibujar();
  }

  // ─────────────────────────────────────────────────────────────── el bucle

  private asentar(): void {
    // Cambiar de escala reencuadra. La cámara es **el encuadre, no el sitio**:
    // heredar un zoom de 8× del cielo al abrir una lente nueva dejaría la
    // lente fuera de la pantalla, mirando a un punto que ya no existe.
    this.aEncuadre(true);
    this.onCamara(false);
    this.señalar(null);
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
    // respiración, ni a media entrada, ni a mitad de un acercamiento.
    this.asiento = 1;
    this.cam = { ...this.destino };
    this.dibujar();
  }

  private raton(e: PointerEvent): void {
    if (this.arrastre) {
      const dx = e.clientX - this.arrastre.sx;
      const dy = e.clientY - this.arrastre.sy;
      this.arrastre.mov = Math.max(this.arrastre.mov, Math.hypot(dx, dy));
      this.destino.x = this.arrastre.cx - dx / this.cam.z;
      this.destino.y = this.arrastre.cy - dy / this.cam.z;
      // **El arrastre es 1:1 y sin retardo.** Interpolarlo se sentiría como
      // arrastrar el mapa con una goma, y I10 dice que la animación no está en
      // el camino de la información: aquí la información es dónde está tu mano.
      this.cam.x = this.destino.x;
      this.cam.y = this.destino.y;
      // Y el paralaje **se congela mientras arrastras**: persigue al ratón, así
      // que sumado al arrastre movería el mapa un poco más que la mano y el 1:1
      // dejaría de ser cierto justo donde se nota.
      this.onCamara(this.movida);
      this.despertar();
      if (!this.corriendo) this.dibujar();
      return;
    }
    const [w, h] = this.medidas();
    // Profundidad simulada (§7.5): las capas se desplazan distinto. Es el 3D
    // que este grafo admite — el otro, el de verdad, hace la topología ilegible
    // por oclusión y ambigüedad de profundidad.
    this.paralaje.tx = ((e.clientX - w / 2) / w) * -18;
    this.paralaje.ty = ((e.clientY - h / 2) / h) * -18;
    this.señalar(this.cuerpoEn(e.clientX, e.clientY));
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
    if (!this.arrastre) {
      this.paralaje.x += (this.paralaje.tx - this.paralaje.x) * 0.08;
      this.paralaje.y += (this.paralaje.ty - this.paralaje.y) * 0.08;
    }
    this.rodar(this.app.ticker.deltaMS);
    this.latir(ahora);
    this.app.render();
  };

  /**
   * La cámara se acerca a su destino. **Nada se teletransporta** (§3, #2).
   *
   * El zoom se interpola en logaritmo porque el zoom es multiplicativo: ir de
   * 1× a 4× y de 4× a 16× son el mismo gesto, y una interpolación lineal
   * haría el primero lento y el segundo brusco.
   *
   * Que esto sea animación y no información es exactamente lo que I10 permite:
   * lo que se ve y lo que se puede picar salen los dos de `cam`, que es el
   * valor de **este** fotograma. Nada espera a que termine.
   */
  private rodar(ms: number): void {
    const k = Math.min(1, (Math.min(50, ms) / 1000) * 5.4);
    this.cam.x += (this.destino.x - this.cam.x) * k;
    this.cam.y += (this.destino.y - this.cam.y) * k;
    this.cam.z *= Math.pow(this.destino.z / this.cam.z, k);
    const lejos =
      Math.abs(this.cam.z / this.destino.z - 1) > 0.0025 ||
      Math.hypot(this.destino.x - this.cam.x, this.destino.y - this.cam.y) * this.cam.z > 0.7;
    if (!lejos) this.cam = { ...this.destino };
  }

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
    // El tamaño se amortigua con el zoom; la posición no. `mundo` ya multiplica
    // todo por `z`, así que aquí se deshace lo que sobra para dejar `z^0,34`.
    const amort = Math.pow(this.cam.z, AMORTIGUA - 1);
    for (const a of this.astros) {
      const pulso = Math.sin((t / a.periodo) * Math.PI * 2 + a.fase);
      const k = (1 + a.amplitud * pulso) * (0.92 + 0.08 * entrada) * amort;
      a.halo.scale.set(a.base * 3.1 * k);
      a.nucleo.scale.set(a.base * k);
      a.halo.alpha = a.halo.label ? Number(a.halo.label) * (0.85 + 0.15 * pulso) * entrada : 0;
      a.nucleo.alpha = a.nucleo.label ? Number(a.nucleo.label) * entrada : 0;
    }
    this.mundo.alpha = 0.25 + 0.75 * entrada;
    this.capaRealce.alpha = entrada;
    this.componer();

    // Polvo y aristas se repintan **sólo cuando el zoom se ha movido de
    // verdad** — un 4 %—: rehacer tres mil puntos en cada fotograma de un
    // acercamiento es exactamente la batería que §2 dice que no se gasta.
    if (Math.abs(Math.log(this.cam.z / this.zPintado)) > 0.04) this.reescalar();

    // Los nombres y los realces viven en coordenadas de pantalla —ahí es donde
    // se decide si dos nombres chocan—, así que cualquier cosa que mueva la
    // composición los recoloca. Y **el nombre no se despega nunca de su
    // estrella**: sale de la misma `aPantalla` que la dibuja a ella.
    const c = this.cam;
    const a = this.aplicada;
    if (c.x !== a.x || c.y !== a.y || c.z !== a.z || this.paralaje.x !== a.px || this.paralaje.y !== a.py) {
      this.superponer();
    }

    // El fundido de §3: los nombres entran y salen, no parpadean. Con el bucle
    // parado —movimiento reducido o ventana sin foco— se ponen de una vez, que
    // es lo que dice la disciplina: sin animación la aplicación sigue entera.
    const paso = this.corriendo ? 0.18 : 1;
    for (const r of this.rotulos.values()) {
      const meta = r.objetivo * entrada;
      r.txt.alpha += (meta - r.txt.alpha) * paso;
      r.txt.visible = r.txt.alpha > 0.012;
    }
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

  // ────────────────────────────────────────── saber qué vas a abrir (pieza 2)

  /** Lo que se dibuja en pantalla y no en el mundo: los nombres y el realce. */
  private superponer(): void {
    this.aplicada = {
      x: this.cam.x,
      y: this.cam.y,
      z: this.cam.z,
      px: this.paralaje.x,
      py: this.paralaje.y,
    };
    this.rotular();
    this.realzar();
  }

  /**
   * Qué nombres se ven. **La respuesta es la escala, no el volumen.**
   *
   * En una lente de hasta 300 nodos, etiquetarlos todos serían trescientas
   * razones para no mirar el mapa; dejar sólo el foco —que es lo que había—
   * deja 299 puntos sin nombre y se pica a ciegas. La salida es la misma que ya
   * usaba el prototipo: se enseñan **los que caben a la escala a la que estás**,
   * y acercarse revela más porque acercarse separa las estrellas y agranda las
   * pequeñas. No hay un umbral de zoom escrito en ningún sitio: hay un radio en
   * pantalla por debajo del cual un nombre no se gana su sitio, y una caja que
   * ya está ocupada.
   *
   * Por eso esto vive en coordenadas de pantalla: dos nombres chocan o no
   * chocan **a un zoom concreto**, y esa cuenta no se puede hacer en el mundo.
   */
  private rotular(): void {
    const [w, h] = this.medidas();
    const cajas: Array<[number, number, number, number]> = [];
    const puestos = new Set<string>();

    // Lo que está debajo del puntero y lo que está seleccionado van primero y
    // **no piden permiso**: son la respuesta a «¿qué voy a abrir?», y el resto
    // de los nombres les cede el sitio.
    const orden = this.cuerpos.filter((c) => c.id === this.encima?.id || c.id === this.elegido);
    orden.push(...this.cuerpos);

    for (const c of orden) {
      if (puestos.has(c.id)) continue;
      const obligado = c.id === this.encima?.id || c.id === this.elegido;
      const [x, y] = this.aPantalla(c.x, c.y);
      if (x < -180 || x > w + 180 || y < -40 || y > h + 40) continue;
      const rp = Math.max(3, this.radioVisual(c.radio));
      if (!obligado && (rp < RADIO_ROTULO || puestos.size >= ROTULOS_MAX)) continue;

      const r = this.etiqueta(c);
      const cy = y + rp + 7;
      const caja: [number, number, number, number] = [
        x - r.txt.width / 2 - 4,
        cy - 2,
        x + r.txt.width / 2 + 4,
        cy + r.txt.height + 2,
      ];
      // Una etiqueta encima de otra no es que se lea mal: es que **no se lee
      // ninguna de las dos**, y dos nombres ilegibles informan menos que un
      // nombre y un punto sin él.
      const choca = cajas.some(
        (o) => caja[0] < o[2] && caja[2] > o[0] && caja[1] < o[3] && caja[3] > o[1],
      );
      if (choca && !obligado) continue;
      cajas.push(caja);
      r.txt.position.set(x, cy);
      // §1 otra vez: la luminosidad es relevancia. Un nombre apagado es una
      // región que la consulta no ha encendido, no un nombre menos importante.
      r.objetivo = obligado ? 1 : c.vivo ? 0.88 : 0.16;
      puestos.add(c.id);
    }

    for (const [id, r] of this.rotulos) if (!puestos.has(id)) r.objetivo = 0;
  }

  /** Un nombre, creado la primera vez que se gana su sitio y reutilizado
   *  después: fabricar `Text` en cada fotograma sería pintar tipografía sesenta
   *  veces por segundo para leerla una. */
  private etiqueta(c: Picable): Rotulo {
    let r = this.rotulos.get(c.id);
    if (!r) {
      const txt = new Text({ text: c.texto, style: ESTILO_ROTULO });
      txt.anchor.set(0.5, 0);
      txt.alpha = 0;
      txt.visible = false;
      this.capaTexto.addChild(txt);
      r = { txt, objetivo: 0 };
      this.rotulos.set(c.id, r);
    }
    return r;
  }

  /**
   * El anillo de «esto es lo que vas a abrir» y el de «esto es lo que estás
   * mirando». Son dos estados distintos y se ven distintos a propósito: el
   * primero sigue al ratón, el segundo se queda después de abrir el panel.
   *
   * Y sí, es luz que significa algo (§1): dice cuál de los trescientos puntos
   * responde si pulsas ahora.
   */
  private realzar(): void {
    const g = this.realce;
    g.clear();
    const anillo = (c: Picable, extra: number, color: number, alpha: number, grosor: number) => {
      const [x, y] = this.aPantalla(c.x, c.y);
      g.circle(x, y, Math.max(5, this.radioVisual(c.radio)) + extra).stroke({
        color,
        alpha,
        width: grosor,
      });
    };
    const sel = this.elegido ? this.cuerpos.find((c) => c.id === this.elegido) : undefined;
    if (sel) anillo(sel, 5, 0x9fc0e8, 0.7, 1.4);
    if (this.encima) anillo(this.encima, this.encima.id === this.elegido ? 10 : 7, 0xcfe0f6, 0.36, 1);
  }

  /**
   * Lo que hay que rehacer cuando el zoom cambia de verdad.
   *
   * El polvo y las aristas no se pueden dejar escalar con la cámara: un punto
   * de polvo a 8× sería una mancha de ocho píxeles y una arista de un píxel,
   * una tubería. Y no se puede rehacer en cada fotograma, así que se rehace por
   * pasos del 4 %, que es cuando se empieza a notar.
   */
  private reescalar(): void {
    this.zPintado = this.cam.z;
    const amort = Math.pow(this.cam.z, AMORTIGUA - 1);
    for (const p of this.polvo) p.sp.scale.set(p.base * amort);
    this.dibujarAristas();
  }

  // ──────────────────────────────────────────────────────────── construcción

  private recolocar(): void {
    const [w, h] = this.medidas();
    // `mundo` no se coloca aquí: lo coloca `componer`, que es el único sitio
    // donde se compone. Dos sitios y ya sabemos cómo acaba.
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
    this.polvo = [];
    this.cuerpos = [];
    this.rotulos.clear();
    this.encima = null;
    this.realce.clear();
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
    // El grande se queda el sitio cuando dos nombres chocan, y da igual en qué
    // orden los devuelva el núcleo.
    this.cuerpos.sort((a, b) => b.peso - a.peso);
    this.dibujarAristas();
    this.zPintado = this.cam.z;
    this.superponer();
  }

  /**
   * Las aristas, aparte del resto. Se rehacen al cambiar el zoom porque su
   * grosor **no puede escalar con la cámara**: §1 le tiene asignado un
   * significado al trazo —sólido es hecho, punteado es inferencia— y a 8× una
   * línea de un píxel sería una tubería y el punteado, una fila de ladrillos.
   */
  private dibujarAristas(): void {
    this.capaAristas.removeChildren().forEach((c) => c.destroy());
    const amort = Math.pow(this.cam.z, AMORTIGUA - 1);
    const trazos = new Graphics();
    if (this.cieloActual) this.puentes(trazos, this.cieloActual, amort);
    else if (this.lenteActual) this.relaciones(trazos, this.lenteActual, amort);
    trazos.blendMode = "add";
    this.capaAristas.addChild(trazos);
  }

  /** Curvas, no rectas: una recta se lee como diagrama y una curva como órbita,
   *  y esto es un cielo. */
  private puentes(trazos: Graphics, c: Cielo, amort: number): void {
    const hayFoco = this.encendidas.size > 0;
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
        width: Math.min(2.6, 0.5 + b.weight * 0.22) * amort,
      });
    }
  }

  private construirCielo(c: Cielo): void {
    const hayFoco = this.encendidas.size > 0;
    const presupuesto = Math.min(POLVO_MAX, c.covered);
    const escala = c.covered > 0 ? presupuesto / c.covered : 0;
    const amort = Math.pow(this.cam.z, AMORTIGUA - 1);
    const cortos = nombresCortos(c.regions);

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
        const base = ((1.1 + azarEstable(`${r.id}:${i}:s`) * 1.5) / 64) * 6;
        s.scale.set(base * amort);
        s.tint = color;
        s.alpha = (0.1 + azarEstable(`${r.id}:${i}:o`) * 0.28) * brillo;
        s.blendMode = "add";
        this.capaPolvo.addChild(s);
        this.polvo.push({ sp: s, base });
      }

      this.astro(p.x, p.y, rad, color, brillo, temp, r.id);
      this.cuerpos.push({
        id: r.id,
        x: p.x,
        y: p.y,
        radio: rad,
        texto: cortos.get(r.id) ?? r.label,
        peso: r.weight,
        vivo: viva,
        fuente: { tipo: "region", region: r },
      });
    }
  }

  private construirLente(l: Lente): void {
    const tiempos = l.nodes.map((n) => n.recency).filter((t) => t > 0);
    const oldest = tiempos.length ? Math.min(...tiempos) : 0;
    const newest = tiempos.length ? Math.max(...tiempos) : 0;

    for (const n of l.nodes) {
      const p = this.pos.get(n.id);
      if (!p) continue;
      const temp = calor(n.recency, oldest, newest);
      const color = n.sensitivity === "vault" ? mezclar(VAULT, VAULT, 0) : mezclar(FRIO, CALIDO, temp);
      const rad = radioNodo(n.degree);
      this.astro(p.x, p.y, rad, color, luz(n.distance), temp, n.id);
      this.cuerpos.push({
        id: n.id,
        x: p.x,
        y: p.y,
        radio: rad,
        texto: n.label,
        // El foco por delante de todo: es de lo que cuelga el resto de la
        // lente, y su nombre es el que contesta «¿dónde estoy?».
        peso: n.distance === 0 ? Infinity : n.degree,
        vivo: true,
        fuente: { tipo: "nodo", nodo: n },
      });
    }
  }

  private relaciones(trazos: Graphics, l: Lente, amort: number): void {
    for (const e of l.edges) {
      const a = this.pos.get(e.from);
      const b = this.pos.get(e.to);
      if (!a || !b) continue;
      const alfa = 0.1 + e.confidence * 0.26;
      if (solida(e)) {
        trazos.moveTo(a.x, a.y).lineTo(b.x, b.y);
        trazos.stroke({ color: 0x8fb2dd, alpha: alfa, width: amort });
      } else {
        // Punteada a mano: Pixi no trae trazo discontinuo, y la distinción
        // hecho/inferencia de §1 no se negocia por una limitación de la
        // librería de dibujo. El paso va en píxeles de pantalla por lo mismo:
        // un punteado que se estira con el zoom deja de leerse como punteado.
        punteada(trazos, a.x, a.y, b.x, b.y, 7 / this.cam.z);
        trazos.stroke({ color: 0x8fb2dd, alpha: alfa * 0.85, width: amort });
      }
    }
  }
}

function punteada(
  g: Graphics,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  paso: number,
): void {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const n = Math.max(1, Math.floor(Math.hypot(dx, dy) / paso));
  for (let i = 0; i < n; i += 2) {
    const t0 = i / n;
    const t1 = Math.min(1, (i + 1) / n);
    g.moveTo(x1 + dx * t0, y1 + dy * t0).lineTo(x1 + dx * t1, y1 + dy * t1);
  }
}
