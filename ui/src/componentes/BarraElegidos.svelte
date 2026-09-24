<!--
  La barra de «varios elegidos» (ADR-0031): aparece abajo cuando eliges más
  de un documento con Ctrl+clic, Mayús+clic o Ctrl+A, en el explorador o en
  una colección.

  Desde aquí se meten todos en una colección de golpe —una carpeta entera es
  Ctrl+A y esto—, y dentro de una colección, se sacan. Nada se mueve de su
  carpeta. `Esc` o la ✕ sueltan la selección.
-->
<script lang="ts">
  import LayersPlus from "@lucide/svelte/icons/layers-plus";
  import ListMinus from "@lucide/svelte/icons/list-minus";
  import X from "@lucide/svelte/icons/x";
  import ElegirColeccion from "./ElegirColeccion.svelte";
  import { app } from "../lib/app.svelte";

  let menu = $state(false);

  const n = $derived(app.elegidos.length);
  const abierta = $derived(app.vista === "coleccion" ? app.abierta : null);
</script>

<div class="barra entra" role="toolbar" aria-label="Documentos elegidos">
  <span class="cuantos">{n === 1 ? "1 elegido" : `${n} elegidos`}</span>
  <div class="ancla">
    <button class="accion hov-fondo" onclick={() => (menu = !menu)} aria-haspopup="menu" aria-expanded={menu}>
      <LayersPlus size={16} />
      <span>Añadir a una colección</span>
    </button>
    {#if menu}
      <ElegirColeccion docs={[...app.elegidos]} hacia="arriba" oncerrar={() => (menu = false)} />
    {/if}
  </div>
  {#if abierta}
    {@const id = abierta.id}
    <button class="accion hov-fondo" onclick={() => app.sacar(id, [...app.elegidos])}>
      <ListMinus size={16} />
      <span>Sacar de «{abierta.name}»</span>
    </button>
  {/if}
  <button class="soltar hov-fondo" onclick={() => app.soltarElegidos()} aria-label="Soltar la selección" title="Soltar · Esc">
    <X size={16} />
  </button>
</div>

<style>
  /* Centrada sin `transform`: un antepasado transformado haría que la
     cortina del menú (fija) cubriera sólo la barra y no la ventana. */
  .barra {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 22px;
    width: max-content;
    margin-inline: auto;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 6px 6px 16px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--linea);
    box-shadow: 0 18px 50px -16px var(--sombra);
    white-space: nowrap;
  }
  .cuantos {
    font-weight: 600;
    margin-right: 6px;
    font-variant-numeric: tabular-nums;
  }
  .ancla {
    position: relative;
  }
  .accion {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: 10px;
    color: var(--acento);
    font-weight: 500;
  }
  .soltar {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    color: var(--tenue);
  }
  .ancla :global(.menu) {
    left: -40px;
  }
</style>
