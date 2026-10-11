<!--
  Un apartado de la vista previa que se pliega: su título es un botón, con la
  flecha de «Detalles técnicos». Lo que pliegas se queda plegado al abrir otros
  documentos y al cerrar la app (JP, 2026-09-23: «que los apartados sean
  clicables para que se muestren o no se muestren»).

  Plegado, el título sigue ahí —con cuántas cosas hay dentro, si se sabe—:
  nada desaparece sin dejar dónde volver a abrirlo.
-->
<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import type { Snippet } from "svelte";
  import { app } from "../lib/app.svelte";

  let {
    id,
    titulo,
    deFabrica = true,
    cuenta,
    tenue = false,
    children,
  }: {
    /** Con qué nombre se recuerda: `documento`, `sabido`, `relacionado`… */
    id: string;
    titulo: string;
    /** Cómo nace, mientras no lo cambies. */
    deFabrica?: boolean;
    /** Cuántas cosas hay dentro, para decirlo plegado. */
    cuenta?: number;
    /** El título en pequeño y sin mayúsculas, como «Detalles técnicos». */
    tenue?: boolean;
    children: Snippet;
  } = $props();

  const abierto = $derived(app.abierto(id, deFabrica));
  const cuerpo = $derived(`apartado-${id}`);
</script>

<section class="apartado" aria-label={titulo}>
  <h3>
    <button
      class="titulo"
      class:rotulo={!tenue}
      class:tenue
      aria-expanded={abierto}
      aria-controls={cuerpo}
      onclick={() => app.ponerApartado(id, !abierto)}
    >
      <span class="flecha" class:abierta={abierto}><ChevronRight size={14} /></span>
      {titulo}
      {#if !abierto && cuenta}<span class="cuenta">{cuenta}</span>{/if}
    </button>
  </h3>
  {#if abierto}
    <div class="cuerpo" id={cuerpo}>
      {@render children()}
    </div>
  {/if}
</section>

<style>
  .apartado {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 0;
    font: inherit;
  }
  .titulo {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: -4px;
    padding: 2px 4px;
    border-radius: 6px;
    text-align: left;
  }
  .titulo:hover {
    color: var(--tinta);
  }
  .titulo:focus-visible {
    outline: 2px solid var(--acento);
    outline-offset: 1px;
  }
  .tenue {
    font-size: 12.5px;
    color: var(--tenue);
  }
  .flecha {
    display: grid;
    transition: transform 0.15s ease;
  }
  .flecha.abierta {
    transform: rotate(90deg);
  }
  .cuenta {
    padding: 0 7px;
    border-radius: 999px;
    background: var(--chip);
    font-size: 11px;
    letter-spacing: 0;
  }
  .cuerpo {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  @media (prefers-reduced-motion: reduce) {
    .flecha {
      transition: none;
    }
  }
</style>
