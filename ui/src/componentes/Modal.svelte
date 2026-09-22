<!--
  Una ventana encima de todo, con el mismo aspecto que el buscador: la
  cortina oscurece el escritorio y `Esc` cierra (si se puede cerrar).

  La usan la clave de recuperación, el PIN y el panel de Seguridad (ADR-0021).
  Sin `cerrar`, no se cierra con `Esc` ni pulsando fuera: es para lo que hay
  que leer antes de seguir.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import X from "@lucide/svelte/icons/x";

  let {
    titulo,
    cerrar,
    ancho = 480,
    children,
  }: { titulo: string; cerrar?: () => void; ancho?: number; children: Snippet } = $props();

  function tecla(e: KeyboardEvent): void {
    if (e.key === "Escape" && cerrar) {
      e.stopPropagation();
      cerrar();
    }
  }
</script>

<svelte:window onkeydown={tecla} />

<div class="fondo">
  <button class="cortina" onclick={() => cerrar?.()} aria-label="Cerrar" tabindex="-1"></button>
  <div class="modal entra" role="dialog" aria-modal="true" aria-label={titulo} style:width="min({ancho}px, calc(100vw - 32px))">
    <header>
      <h2 class="titular">{titulo}</h2>
      {#if cerrar}
        <button class="cerrar hov-fondo" onclick={cerrar} aria-label="Cerrar" title="Cerrar · Esc">
          <X size={16} />
        </button>
      {/if}
    </header>
    <div class="cuerpo">
      {@render children()}
    </div>
  </div>
</div>

<style>
  .fondo {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    place-items: center;
  }
  .cortina {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: default;
    background: rgba(4, 7, 12, 0.55);
  }
  .modal {
    position: relative;
    max-height: calc(100vh - 48px);
    overflow-y: auto;
    border-radius: 18px;
    background: var(--panel);
    border: 1px solid var(--linea);
    box-shadow: 0 40px 120px -20px rgba(0, 0, 0, 0.6);
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 22px 18px 0 26px;
  }
  h2 {
    flex-grow: 1;
    margin: 0;
    font-size: 30px;
    line-height: 1.1;
  }
  .cerrar {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    color: var(--tenue);
  }
  .cuerpo {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px 26px 24px;
  }
</style>
