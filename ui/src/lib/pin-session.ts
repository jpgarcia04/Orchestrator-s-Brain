/** Una concesión corta y revocable. El reloj es monotónico y sustituible en pruebas. */
export interface PinState {
  pin: boolean;
  desbloqueada: boolean;
  restante_ms: number | null;
}

export class PinSession {
  epoch = 0;
  state: PinState | null = null;
  held = false;
  private deadline = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private listeners = new Set<() => void>();
  private now: () => number;
  private schedule: typeof setTimeout;
  private cancel: typeof clearTimeout;

  constructor(now = () => performance.now(), schedule = setTimeout, cancel = clearTimeout) {
    this.now = now;
    this.schedule = schedule;
    this.cancel = cancel;
  }

  subscribe(f: () => void): () => void {
    this.listeners.add(f);
    return () => this.listeners.delete(f);
  }

  private changed(): void {
    this.epoch++;
    for (const f of this.listeners) f();
  }

  suspend(): void {
    this.cancel(this.timer);
    this.state = null;
    this.deadline = 0;
    this.changed();
  }

  beginChange(): void {
    this.held = true;
    this.suspend();
  }

  finishChange(): void {
    this.held = false;
  }

  apply(state: PinState, elapsed = 0): void {
    if (this.held) return;
    const old = this.state;
    if (state.pin && state.desbloqueada) {
      const remaining = Math.max(0, (state.restante_ms ?? 0) - elapsed);
      const deadline = this.now() + remaining;
      // Una respuesta atrasada nunca alarga la misma concesión.
      this.deadline = old?.pin && old.desbloqueada ? Math.min(this.deadline, deadline) : deadline;
      if (this.deadline <= this.now()) {
        this.suspend();
        return;
      }
      this.cancel(this.timer);
      this.timer = this.schedule(() => this.suspend(), this.deadline - this.now());
    } else {
      this.deadline = 0;
      this.cancel(this.timer);
    }
    this.state = state;
    if (!old || old.pin !== state.pin || old.desbloqueada !== state.desbloqueada) this.changed();
  }

  current(epoch: number): boolean {
    if (this.state?.pin && this.state.desbloqueada && this.now() >= this.deadline) this.suspend();
    return !this.held && this.state !== null && epoch === this.epoch;
  }

  /** Comprueba antes y después de una lectura, incluida la consulta de seguridad.
   * El error/resultado tardío se descarta aunque ya haya otro desbloqueo. */
  async read<T>(request: () => Promise<T>, security: () => Promise<PinState>): Promise<T> {
    const epoch = this.epoch;
    if (!this.current(epoch)) throw new Error("No se pudo comprobar la seguridad");
    const value = await request();
    if (!this.current(epoch)) throw new Error("Lectura revocada");
    const started = this.now();
    let state: PinState;
    try {
      state = await security();
    } catch (e) {
      if (epoch === this.epoch) this.suspend();
      throw e;
    }
    if (!this.current(epoch)) throw new Error("Lectura revocada");
    this.apply(state, this.now() - started);
    if (!this.current(epoch)) throw new Error("Lectura revocada");
    return value;
  }
}

export const pinSession = new PinSession();
