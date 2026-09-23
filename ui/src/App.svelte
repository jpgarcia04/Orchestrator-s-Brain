<!--
  El escritorio: la barra lateral, la cabecera, la vista que toque, la vista
  previa y el buscador.

  El explorador es donde se trabaja a diario y la galaxia es la vista «Mapa»,
  enlazada con él en los dos sentidos (ADR-0019, confirmado por el usuario).
  Las teclas de siempre:

  - `Ctrl K`, o escribir en cualquier sitio: buscar (ADR-0017: escribir abre).
  - `Esc`: deshace un escalón — el buscador, la vista previa, la lente del mapa.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import BarraLateral from "./componentes/BarraLateral.svelte";
  import Buscador from "./componentes/Buscador.svelte";
  import Cabecera from "./componentes/Cabecera.svelte";
  import Pin from "./componentes/Pin.svelte";
  import Recuperacion from "./componentes/Recuperacion.svelte";
  import Seguridad from "./componentes/Seguridad.svelte";
  import VistaPrevia from "./componentes/VistaPrevia.svelte";
  import { app } from "./lib/app.svelte";
  import Carpeta from "./vistas/Carpeta.svelte";
  import Cerrada from "./vistas/Cerrada.svelte";
  import Inicio from "./vistas/Inicio.svelte";
  import Mapa from "./vistas/Mapa.svelte";
  import Papelera from "./vistas/Papelera.svelte";
  import Sensibles from "./vistas/Sensibles.svelte";

  $effect(() => {
    document.documentElement.dataset["tema"] = app.tema;
  });

  onMount(() => {
    void app.arrancar();
  });

  function escribiendo(t: EventTarget | null): boolean {
    return (
      t instanceof HTMLInputElement ||
      t instanceof HTMLTextAreaElement ||
      (t instanceof HTMLElement && t.isContentEditable)
    );
  }

  function tecla(e: KeyboardEvent): void {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.abrirBuscador();
      return;
    }
    if (app.buscando || app.enModal || app.info?.cerrada || escribiendo(e.target)) return; // tienen sus teclas
    if (e.key === "Escape") {
      if (app.seleccion) app.cerrarPanel();
      else if (app.vista === "mapa" && app.lente) app.lente = null;
      return;
    }
    // Escribir abre el buscador, y la letra no se pierde.
    if (!e.ctrlKey && !e.metaKey && !e.altKey && e.key.length === 1 && e.key !== " ") {
      e.preventDefault();
      app.abrirBuscador(e.key);
    }
  }
</script>

<svelte:window onkeydown={tecla} />

{#if app.info?.cerrada}
  <Cerrada />
{:else}
<div class="escritorio">
  <BarraLateral />

  <div class="centro">
    <Cabecera />
    <div class="cuerpo">
      <main>
        {#if app.error}
          <p class="error">{app.error}</p>
        {/if}
        {#if app.vista === "inicio"}
          <Inicio />
        {:else if app.vista === "carpeta"}
          <Carpeta />
        {:else if app.vista === "mapa"}
          <Mapa />
        {:else if app.vista === "papelera"}
          <Papelera />
        {:else}
          <Sensibles />
        {/if}
      </main>

      {#if app.seleccion}
        {#key app.seleccion}
          <VistaPrevia id={app.seleccion} consulta={app.consulta} />
        {/key}
      {/if}
    </div>
  </div>

  {#if app.buscando}
    <Buscador />
  {/if}

  {#if app.mostrarRecuperacion}
    <Recuperacion />
  {/if}
  {#if app.panelSeguridad}
    <Seguridad />
  {/if}
  {#if app.pidiendoPin}
    <Pin />
  {/if}
</div>
{/if}

<div class="avisos" aria-live="polite">
  {#if app.aviso}
    <p class="aviso entra">{app.aviso}</p>
  {/if}
</div>

<style>
  .escritorio {
    display: flex;
    height: 100%;
    min-width: 900px;
  }

  .centro {
    flex-grow: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .cuerpo {
    flex-grow: 1;
    min-height: 0;
    display: flex;
  }

  main {
    position: relative;
    flex-grow: 1;
    min-width: 0;
    overflow-y: auto;
  }

  .error {
    margin: 20px 36px 0;
    color: var(--peligro);
  }

  .avisos {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 26px;
    display: flex;
    justify-content: center;
    pointer-events: none;
    z-index: 30;
  }
  .aviso {
    max-width: 640px;
    padding: 11px 18px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    box-shadow: 0 18px 50px -16px rgba(0, 0, 0, 0.55);
    font-size: 13.5px;
  }
</style>
