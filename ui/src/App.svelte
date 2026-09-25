<!--
  El escritorio: la barra lateral, la cabecera, la vista que toque, la vista
  previa y el buscador.

  El explorador es donde se trabaja a diario y la galaxia es la vista «Mapa»,
  enlazada con él en los dos sentidos (ADR-0019, confirmado por el usuario).
  Las teclas de siempre:

  - `Ctrl K`, o escribir en cualquier sitio: buscar (ADR-0017: escribir abre).
  - `Esc`: deshace un escalón — el buscador, lo elegido, la vista previa, la
    lente del mapa.
  - `Ctrl A`, en una carpeta o una colección: elegir todos sus documentos
    (ADR-0031).
  - Arrastrar un documento (o los elegidos) a una colección de la barra
    lateral: desde una carpeta añade, desde una colección mueve, y con `Ctrl`
    añade; `Esc` lo cancela (ADR-0033).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Arrastrando from "./componentes/Arrastrando.svelte";
  import BarraElegidos from "./componentes/BarraElegidos.svelte";
  import BarraLateral from "./componentes/BarraLateral.svelte";
  import Buscador from "./componentes/Buscador.svelte";
  import Cabecera from "./componentes/Cabecera.svelte";
  import Pin from "./componentes/Pin.svelte";
  import Recuperacion from "./componentes/Recuperacion.svelte";
  import Seguridad from "./componentes/Seguridad.svelte";
  import VistaPrevia from "./componentes/VistaPrevia.svelte";
  import { app } from "./lib/app.svelte";
  import Carpeta from "./vistas/Carpeta.svelte";
  import Coleccion from "./vistas/Coleccion.svelte";
  import Colecciones from "./vistas/Colecciones.svelte";
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
    if (app.arrastre) {
      // Arrastrando, `Esc` lo cancela y las demás teclas no hacen nada (Ctrl
      // cambia «Mover» por «Añadir»; lo mira el propio arrastre).
      if (e.key === "Escape") app.cancelarArrastre();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.abrirBuscador();
      return;
    }
    if (app.buscando || app.enModal || app.menu || app.info?.cerrada || escribiendo(e.target)) return; // tienen sus teclas
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && app.enLista) {
      e.preventDefault();
      app.elegirTodos(app.ordenLista);
      return;
    }
    if (e.key === "Escape") {
      if (app.elegidos.length) app.soltarElegidos();
      else if (app.seleccion) app.cerrarPanel();
      else if (app.vista === "mapa" && (app.lente || app.lenteColeccion)) {
        app.lente = null;
        app.lenteColeccion = null;
      }
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
      <div class="zona">
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
          {:else if app.vista === "colecciones"}
            <Colecciones />
          {:else if app.vista === "coleccion"}
            <Coleccion />
          {:else}
            <Sensibles />
          {/if}
        </main>
        {#if app.elegidos.length > 0 && app.enLista}
          <BarraElegidos />
        {/if}
      </div>

      {#if app.seleccion}
        {#key app.seleccion}
          <VistaPrevia id={app.seleccion} consulta={app.consulta} tramo={app.tramo} />
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
  <Arrastrando />
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

  /* La vista y, encima de ella abajo, la barra de lo elegido: la barra no se
     desplaza con la lista. */
  .zona {
    position: relative;
    flex-grow: 1;
    min-width: 0;
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
