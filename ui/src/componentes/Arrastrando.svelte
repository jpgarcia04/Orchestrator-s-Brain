<!--
  La etiqueta que sigue al ratón mientras arrastras a una colección
  (ADR-0033): qué llevas y **qué va a pasar si sueltas ahí** —«Añadir a…»,
  «Mover a…», «Ya está en…»—, antes de soltar. No se interpone: el ratón la
  atraviesa.
-->
<script lang="ts">
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import FileText from "@lucide/svelte/icons/file-text";
  import Files from "@lucide/svelte/icons/files";
  import Plus from "@lucide/svelte/icons/plus";
  import { app } from "../lib/app.svelte";

  const a = $derived(app.arrastre);
  const hara = $derived(app.queHara);

  /**
   * Junto al ratón; y sobre la barra lateral, **a su lado**, a la altura del
   * ratón: así no tapa las colecciones de alrededor, que son donde se suelta.
   */
  const pos = $derived.by(() => {
    if (!a) return { x: 0, y: 0 };
    const borde = document.querySelector("nav.lateral")?.getBoundingClientRect().right ?? 0;
    return a.x < borde ? { x: borde + 10, y: a.y - 24 } : { x: a.x + 16, y: a.y + 14 };
  });
</script>

{#if a}
  <div class="fantasma" style:transform="translate({pos.x}px, {pos.y}px)" aria-hidden="true">
    <span class="que">
      {#if a.docs.length > 1}<Files size={15} />{:else}<FileText size={15} />{/if}
      <span class="nombre">{a.etiqueta}</span>
    </span>
    <span class="pasa" class:vale={hara.vale}>
      {#if hara.vale}
        {#if app.mueve(a)}<ArrowRight size={13} />{:else}<Plus size={13} />{/if}
      {/if}
      {hara.texto}
    </span>
  </div>
{/if}

<style>
  .fantasma {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 40;
    pointer-events: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 300px;
    padding: 9px 13px;
    border-radius: 12px;
    background: var(--panel);
    border: 1px solid var(--linea);
    box-shadow: 0 18px 50px -16px var(--sombra);
  }
  .que {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
    min-width: 0;
  }
  .que :global(svg) {
    flex-shrink: 0;
    color: var(--tenue);
  }
  .nombre {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pasa {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--tenue);
  }
  .pasa.vale {
    color: var(--acento);
  }
</style>
