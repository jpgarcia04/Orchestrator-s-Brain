<!--
  La clave de recuperación, la primera vez (ADR-0021).

  Sale al abrir la app en cuanto la base se cifra, y vuelve a salir en cada
  arranque hasta que digas que la guardaste. No se cierra con `Esc` ni
  pulsando fuera: es lo único de la app que hay que leer antes de seguir.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { pinSession } from "../lib/pin-session";
  import Check from "@lucide/svelte/icons/check";
  import Copy from "@lucide/svelte/icons/copy";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Modal from "./Modal.svelte";
  import { app } from "../lib/app.svelte";
  import { frase } from "../lib/nombres";
  import { claveRecuperacion, recuperacionGuardada } from "../lib/nucleo";

  let clave = $state<string | null>(null);
  let guardada = $state(false);
  let copiada = $state(false);
  onMount(() => pinSession.subscribe(() => { clave = null; copiada = false; }));
  let error = $state("");

  claveRecuperacion()
    .then((c) => (clave = c))
    .catch((e) => (error = frase(e)));

  async function copiar(): Promise<void> {
    if (!clave) return;
    try {
      await navigator.clipboard.writeText(clave);
      copiada = true;
      window.setTimeout(() => (copiada = false), 2500);
    } catch {
      error = "No se pudo copiar: selecciónala y cópiala a mano.";
    }
  }

  async function listo(): Promise<void> {
    try {
      await recuperacionGuardada();
      await app.recargarSeguridad();
      app.avisar("Listo. Puedes volver a verla en «Seguridad», abajo a la izquierda.");
    } catch (e) {
      error = frase(e);
    }
  }
</script>

<Modal titulo="Tu base ya está cifrada" ancho={540}>
  <p class="intro">
    <ShieldCheck size={18} />
    <span>
      Todo lo que Orchestrator's Brain sabe de tus documentos se guarda cifrado en este equipo, y la llave
      la guarda Windows. Si algún día Windows la pierde —otro equipo, o Windows reinstalado—, la base sólo
      se abre con esta clave:
    </span>
  </p>

  {#if clave}
    <div class="clave">
      <code>{clave}</code>
      <button class="boton-sec hov" onclick={copiar}>
        {#if copiada}<Check size={15} /> Copiada{:else}<Copy size={15} /> Copiar{/if}
      </button>
    </div>
  {:else if !error}
    <div class="clave"><code class="cargando">·····-·····-·····-·····-·····</code></div>
  {/if}

  <p class="tenue">
    Guárdala en tu gestor de contraseñas o apúntala en un papel, lejos de la laptop. Sin ella, si Windows
    pierde la llave, tus documentos siguen en tus carpetas, pero perderías tus marcas y correcciones.
  </p>

  <p class="aviso">
    Cifrar la base <strong>no cifra tus archivos originales</strong>. Para eso, activa el Cifrado de
    dispositivo de Windows: Configuración › Privacidad y seguridad › Cifrado de dispositivo.
  </p>

  {#if error}<p class="error-campo">{error}</p>{/if}

  <label class="marca">
    <input type="checkbox" bind:checked={guardada} />
    <span>Ya la guardé en un sitio seguro</span>
  </label>

  <div class="botones">
    <button class="boton-sec hov" onclick={() => (app.recuperacionAplazada = true)}>Más tarde</button>
    <button class="boton-prim hov" disabled={!guardada} onclick={listo}>Listo</button>
  </div>
</Modal>

<style>
  .intro {
    display: flex;
    gap: 12px;
    line-height: 1.55;
  }
  .intro :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--calido);
  }
  .clave {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 14px 14px 18px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  code {
    flex-grow: 1;
    font-family: var(--mono);
    font-size: 17px;
    letter-spacing: 1px;
    white-space: nowrap;
    user-select: all;
  }
  code.cargando {
    color: var(--tenue);
  }
  .tenue {
    font-size: 13.5px;
    line-height: 1.55;
    color: var(--tenue);
  }
  .aviso {
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--calido-fondo);
    font-size: 13px;
    line-height: 1.5;
  }
  .marca {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }
  .marca input {
    width: 16px;
    height: 16px;
    accent-color: var(--prim);
  }
  .botones {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
</style>
