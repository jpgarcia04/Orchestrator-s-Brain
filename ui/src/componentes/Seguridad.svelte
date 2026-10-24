<!--
  El panel de Seguridad (ADR-0021): el cifrado, la clave de recuperación y el
  PIN, en un sitio.

  Dice las cosas como son. Lo que protege el cifrado —una copia de la base no
  se lee— y lo que no —tus archivos originales—. Lo que protege el PIN —ver lo
  sensible dentro de la app— y lo que no —que alguien use tu sesión abierta
  para abrir la carpeta—.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { pinSession } from "../lib/pin-session";
  import Copy from "@lucide/svelte/icons/copy";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Lock from "@lucide/svelte/icons/lock";
  import LockOpen from "@lucide/svelte/icons/lock-open";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Modal from "./Modal.svelte";
  import Respaldos from "./Respaldos.svelte";
  import { app } from "../lib/app.svelte";
  import { frase } from "../lib/nombres";
  import { claveRecuperacion, pinPoner, pinQuitar } from "../lib/nucleo";

  const s = $derived(app.seguridad);

  let pestana = $state<"proteccion" | "respaldos">("proteccion");
  let clave = $state<string | null>(null);
  let copiada = $state(false);
  onMount(() => {
    if (app.seguridad?.reindexado_pausado) pestana = "respaldos";
    return pinSession.subscribe(() => { clave = null; copiada = false; });
  });

  /** Qué formulario del PIN está abierto. */
  let forma = $state<"" | "poner" | "cambiar" | "quitar">("");
  let actual = $state("");
  let nuevo = $state("");
  let repetido = $state("");
  let error = $state("");
  let ocupado = $state(false);

  const soloNumeros = (v: string) => v.replace(/\D/g, "").slice(0, 8);

  function abrirForma(f: typeof forma): void {
    forma = f;
    actual = nuevo = repetido = error = "";
  }

  async function verClave(): Promise<void> {
    error = "";
    try {
      clave = await claveRecuperacion();
    } catch {
      // Con PIN y bloqueado: primero el PIN.
      if (await app.pedirPin()) {
        try {
          clave = await claveRecuperacion();
        } catch (e) {
          error = frase(e);
        }
      }
    }
  }

  async function copiar(): Promise<void> {
    if (!clave) return;
    try {
      await navigator.clipboard.writeText(clave);
      copiada = true;
      window.setTimeout(() => (copiada = false), 2500);
    } catch {
      // se puede seleccionar a mano
    }
  }

  async function guardarPin(e: Event): Promise<void> {
    e.preventDefault();
    if (ocupado) return;
    if (forma !== "quitar") {
      if (nuevo.length < 4) return void (error = "El PIN son de 4 a 8 números.");
      if (nuevo !== repetido) return void (error = "Los dos PIN no coinciden.");
    }
    ocupado = true;
    try {
      if (forma === "quitar") {
        await pinQuitar(actual);
        app.avisar("PIN quitado. Lo sensible vuelve a salir borroso hasta un clic.");
      } else {
        await pinPoner(nuevo, forma === "cambiar" ? actual : null);
        app.avisar(forma === "poner" ? "PIN puesto. Lo sensible te lo pedirá antes de enseñarse." : "PIN cambiado.");
      }
      forma = "";
      await app.recargarSeguridad();
      app.version++;
    } catch (err) {
      error = frase(err);
    } finally {
      ocupado = false;
    }
  }
</script>

<Modal titulo="Seguridad" ancho={pestana === "respaldos" ? 620 : 560} cerrar={() => (app.panelSeguridad = false)}>
  <div class="pestanas-cabecera">
    <button class="boton-pestana" class:activa={pestana === "proteccion"} onclick={() => (pestana = "proteccion")}>
      Protección
    </button>
    <button class="boton-pestana" class:activa={pestana === "respaldos"} onclick={() => (pestana = "respaldos")}>
      Respaldos y restauración
      {#if s?.reindexado_pausado}
        <span class="punto-aviso"></span>
      {/if}
    </button>
  </div>

  {#if pestana === "respaldos"}
    <Respaldos />
  {:else}
  {#if s}
    <section class="bloque">
      <div class="linea">
        {#if s.cifrada && s.proteccion === "dpapi"}
          <ShieldCheck size={20} class="bien" />
          <div>
            <h3>Tu base está cifrada</h3>
            <p>
              Lo que Orchestrator's Brain sabe de tus documentos —el texto que leyó, los nombres, tus marcas— se
              guarda cifrado. La llave la guarda Windows para tu cuenta: una copia de la base no se puede leer.
            </p>
          </div>
        {:else if s.cifrada}
          <ShieldAlert size={20} class="aviso" />
          <div>
            <h3>Cifrada, con la llave sin proteger</h3>
            <p>
              La base está cifrada, pero en este sistema la llave se guarda sin la protección de Windows
              (sólo pasa fuera de Windows, en el banco de pruebas).
            </p>
          </div>
        {:else}
          <ShieldAlert size={20} class="peligro" />
          <div>
            <h3>Tu base no está cifrada</h3>
            <p>Se cifrará la próxima vez que abras la aplicación.</p>
          </div>
        {/if}
      </div>
      <p class="nota">
        Esto <strong>no cifra tus archivos originales</strong>. Para eso: Configuración › Privacidad y
        seguridad › Cifrado de dispositivo.
      </p>
    </section>

    {#if s.cifrada}
      <section class="bloque">
        <div class="linea">
          <KeyRound size={20} />
          <div>
            <h3>Clave de recuperación</h3>
            <p>Abre la base si Windows pierde la llave: otro equipo, o Windows reinstalado.</p>
          </div>
        </div>
        {#if clave}
          <div class="clave">
            <code>{clave}</code>
            <button class="boton-sec hov" onclick={copiar}><Copy size={15} /> {copiada ? "Copiada" : "Copiar"}</button>
          </div>
        {:else}
          <button class="boton-sec hov ancho" onclick={verClave}>Ver la clave</button>
        {/if}
      </section>
    {/if}

    <section class="bloque">
      <div class="linea">
        {#if s.pin && s.desbloqueada}<LockOpen size={20} />{:else}<Lock size={20} />{/if}
        <div>
          <h3>PIN</h3>
          {#if !s.pin}
            <p>
              Sin PIN, lo sensible sale borroso hasta que pulsas «Mostrar». Con PIN, no se enseña —ni en la vista
              previa ni en la búsqueda— hasta escribirlo. No cifra nada: protege ver.
            </p>
          {:else if s.desbloqueada}
            <p>Desbloqueado: lo sensible se ve durante 5 minutos desde que escribes el PIN.</p>
          {:else}
            <p>Bloqueado: lo sensible pide el PIN antes de enseñarse.</p>
          {/if}
        </div>
      </div>

      {#if forma}
        <form class="forma" onsubmit={guardarPin}>
          {#if forma !== "poner"}
            <input
              class="campo"
              type="password"
              inputmode="numeric"
              autocomplete="off"
              placeholder="PIN actual"
              aria-label="PIN actual"
              value={actual}
              oninput={(e) => (actual = soloNumeros(e.currentTarget.value))}
            />
          {/if}
          {#if forma !== "quitar"}
            <input
              class="campo"
              type="password"
              inputmode="numeric"
              autocomplete="off"
              placeholder="PIN nuevo (4 a 8 números)"
              aria-label="PIN nuevo"
              value={nuevo}
              oninput={(e) => (nuevo = soloNumeros(e.currentTarget.value))}
            />
            <input
              class="campo"
              type="password"
              inputmode="numeric"
              autocomplete="off"
              placeholder="Repite el PIN nuevo"
              aria-label="Repite el PIN nuevo"
              value={repetido}
              oninput={(e) => (repetido = soloNumeros(e.currentTarget.value))}
            />
          {/if}
          {#if error}<p class="error-campo">{error}</p>{/if}
          <div class="botones">
            <button type="button" class="boton-sec hov" onclick={() => (forma = "")}>Cancelar</button>
            <button type="submit" class="boton-prim hov" disabled={ocupado}>
              {forma === "poner" ? "Poner PIN" : forma === "cambiar" ? "Cambiar PIN" : "Quitar PIN"}
            </button>
          </div>
        </form>
      {:else}
        <div class="botones izquierda">
          {#if !s.pin}
            <button class="boton-prim hov" onclick={() => abrirForma("poner")}>Poner un PIN</button>
          {:else}
            {#if s.desbloqueada}
              <button class="boton-prim hov" onclick={() => app.bloquear()}><Lock size={15} /> Bloquear ahora</button>
            {/if}
            <button class="boton-sec hov" onclick={() => abrirForma("cambiar")}>Cambiar</button>
            <button class="boton-sec hov" onclick={() => abrirForma("quitar")}>Quitar</button>
          {/if}
        </div>
        {#if error}<p class="error-campo">{error}</p>{/if}
      {/if}
    </section>
  {:else}
    <p class="tenue">No se pudo comprobar la seguridad. El contenido protegido se ha retirado.</p>
    <button class="boton-prim hov" onclick={() => (app.pidiendoPin = true)}>Comprobar con PIN</button>
  {/if}
  {/if}
</Modal>

<style>
  .bloque {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--linea);
  }
  .bloque:last-child {
    padding-bottom: 0;
    border-bottom: 0;
  }
  .linea {
    display: flex;
    gap: 14px;
    color: var(--tenue);
  }
  .linea :global(svg) {
    flex-shrink: 0;
    margin-top: 1px;
  }
  .linea :global(.bien) {
    color: var(--bien);
  }
  .linea :global(.aviso) {
    color: var(--calido);
  }
  .linea :global(.peligro) {
    color: var(--peligro);
  }
  h3 {
    margin-bottom: 3px;
    font-size: 15px;
    font-weight: 600;
    color: var(--tinta);
  }
  .linea p {
    font-size: 13.5px;
    line-height: 1.55;
  }
  .nota {
    margin-left: 34px;
    padding: 9px 12px;
    border-radius: 10px;
    background: var(--calido-fondo);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .clave {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-left: 34px;
    padding: 10px 10px 10px 16px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  code {
    flex-grow: 1;
    font-family: var(--mono);
    font-size: 15px;
    letter-spacing: 0.5px;
    white-space: nowrap;
    user-select: all;
  }
  .ancho {
    align-self: flex-start;
    margin-left: 34px;
  }
  .forma {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-left: 34px;
  }
  .botones {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .botones.izquierda {
    justify-content: flex-start;
  }
  .pestanas-cabecera {
    display: flex;
    gap: 8px;
    margin-bottom: 18px;
    border-bottom: 1px solid var(--linea);
    padding-bottom: 8px;
  }
  .boton-pestana {
    position: relative;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 13.5px;
    font-weight: 500;
    color: var(--tenue);
    background: transparent;
    transition: color 0.15s, background 0.15s;
  }
  .boton-pestana:hover {
    color: var(--tinta);
    background: var(--tarjeta);
  }
  .boton-pestana.activa {
    color: var(--tinta);
    background: var(--tarjeta);
    font-weight: 600;
  }
  .punto-aviso {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--calido);
    margin-left: 6px;
    vertical-align: middle;
    margin-left: 34px;
  }
</style>
