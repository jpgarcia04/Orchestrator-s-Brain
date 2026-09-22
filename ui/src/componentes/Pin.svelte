<!--
  Desbloquear lo sensible con el PIN (ADR-0021).

  El PIN protege **ver**: sin él, el contenido de lo sensible no sale del
  núcleo. Un desbloqueo dura 5 minutos desde la última vez que se mira algo
  sensible. Tras tres fallos, espera creciente, y la espera la guarda la base:
  cerrar la app no la salta. Si se olvidó, la clave de recuperación pone otro.
-->
<script lang="ts">
  import Lock from "@lucide/svelte/icons/lock";
  import Modal from "./Modal.svelte";
  import { app } from "../lib/app.svelte";
  import { frase } from "../lib/nombres";
  import { pinDesbloquear, pinRestablecer } from "../lib/nucleo";

  let modo = $state<"pin" | "olvido">("pin");
  let pin = $state("");
  let codigo = $state("");
  let nuevo = $state("");
  let error = $state("");
  let ocupado = $state(false);
  let campo: HTMLInputElement | undefined = $state();

  $effect(() => {
    void modo;
    campo?.focus();
  });

  const soloNumeros = (s: string) => s.replace(/\D/g, "").slice(0, 8);

  async function desbloquear(e?: Event): Promise<void> {
    e?.preventDefault();
    if (pin.length < 4 || ocupado) return;
    ocupado = true;
    try {
      await pinDesbloquear(pin);
      await app.pinResuelto(true);
    } catch (err) {
      error = frase(err);
      pin = "";
      campo?.focus();
    } finally {
      ocupado = false;
    }
  }

  async function restablecer(e?: Event): Promise<void> {
    e?.preventDefault();
    if (ocupado) return;
    if (nuevo && nuevo.length < 4) {
      error = "El PIN nuevo son de 4 a 8 números.";
      return;
    }
    ocupado = true;
    try {
      await pinRestablecer(codigo, nuevo || null);
      await app.pinResuelto(true);
      app.avisar(nuevo ? "Tu PIN es el nuevo." : "Ya no hay PIN: lo sensible vuelve a salir borroso hasta un clic.");
    } catch (err) {
      error = frase(err);
    } finally {
      ocupado = false;
    }
  }
</script>

<Modal titulo={modo === "pin" ? "Escribe tu PIN" : "¿Olvidaste el PIN?"} ancho={420} cerrar={() => app.pinResuelto(false)}>
  {#if modo === "pin"}
    <form class="forma" onsubmit={desbloquear}>
      <p class="tenue">
        <Lock size={15} /> Lo sensible se queda a la vista 5 minutos desde la última vez que lo miras.
      </p>
      <input
        bind:this={campo}
        class="campo pin"
        type="password"
        inputmode="numeric"
        autocomplete="off"
        placeholder="····"
        aria-label="PIN"
        value={pin}
        oninput={(e) => (pin = soloNumeros(e.currentTarget.value))}
      />
      {#if error}<p class="error-campo">{error}</p>{/if}
      <button class="boton-prim hov" type="submit" disabled={pin.length < 4 || ocupado}>Desbloquear</button>
      <button class="enlace-sec" type="button" onclick={() => ((modo = "olvido"), (error = ""))}>¿Olvidaste el PIN?</button>
    </form>
  {:else}
    <form class="forma" onsubmit={restablecer}>
      <p class="tenue">
        Escribe tu clave de recuperación —la que guardaste al cifrar la base— y, si quieres, un PIN nuevo.
        Sin PIN nuevo, se quita.
      </p>
      <input
        bind:this={campo}
        class="campo clave"
        autocomplete="off"
        spellcheck="false"
        placeholder="XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"
        aria-label="Clave de recuperación"
        bind:value={codigo}
      />
      <input
        class="campo"
        type="password"
        inputmode="numeric"
        autocomplete="off"
        placeholder="PIN nuevo (opcional)"
        aria-label="PIN nuevo"
        value={nuevo}
        oninput={(e) => (nuevo = soloNumeros(e.currentTarget.value))}
      />
      {#if error}<p class="error-campo">{error}</p>{/if}
      <button class="boton-prim hov" type="submit" disabled={codigo.trim().length < 25 || ocupado}>Restablecer</button>
      <button class="enlace-sec" type="button" onclick={() => ((modo = "pin"), (error = ""))}>Volver</button>
    </form>
  {/if}
</Modal>

<style>
  .forma {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .tenue {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    font-size: 13.5px;
    line-height: 1.55;
    color: var(--tenue);
  }
  .tenue :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
  }
  .pin {
    font-family: var(--mono);
    font-size: 22px;
    letter-spacing: 6px;
    text-align: center;
  }
  .pin::placeholder {
    letter-spacing: 6px;
  }
  .clave {
    font-family: var(--mono);
    letter-spacing: 1px;
  }
</style>
