<!--
  La barra lateral: por dónde se entra a todo.

  Arriba, buscar. Después las vistas —el inicio, el mapa, lo sensible— y tu
  bóveda carpeta por carpeta, con su nombre limpio y cuántos documentos tiene.
  Abajo, el estado: si está al día, actualizar, añadir otra carpeta y el
  aspecto.
-->
<script lang="ts">
  import Folder from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import House from "@lucide/svelte/icons/house";
  import Lock from "@lucide/svelte/icons/lock";
  import Moon from "@lucide/svelte/icons/moon";
  import Orbit from "@lucide/svelte/icons/orbit";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import Sun from "@lucide/svelte/icons/sun";
  import { app } from "../lib/app.svelte";
  import { nombreBonito } from "../lib/nombres";
  import { sensibles } from "../lib/nucleo";

  let cuantosSensibles = $state(0);

  $effect(() => {
    void app.version;
    sensibles()
      .then((s) => (cuantosSensibles = s.length))
      .catch(() => (cuantosSensibles = 0));
  });

  const carpetas = $derived((app.raiz?.rows ?? []).filter((r) => r.kind === "folder"));

  /** La carpeta abierta y todo su camino: la de la barra se marca si estás dentro. */
  const abiertas = $derived.by(() => {
    const l = app.vista === "carpeta" ? app.listado : null;
    const ids = new Set<string>(l?.trail.map((m) => m.id) ?? []);
    if (l?.focus) ids.add(l.focus);
    return ids;
  });

  const estado = $derived.by(() => {
    if (app.indexando) return { texto: "Leyendo tu bóveda…", color: "var(--calido)" };
    if (app.vacia) return { texto: "Sin carpetas todavía", color: "var(--tenue)" };
    return { texto: "Al día", color: "var(--bien)" };
  });

  const ruta = $derived(app.info?.libraries.join("\n") ?? "");
</script>

<nav class="lateral" aria-label="Navegación">
  <div class="marca">
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path d="M10 2.5c.6 3.8 3.7 6.9 7.5 7.5-3.8.6-6.9 3.7-7.5 7.5-.6-3.8-3.7-6.9-7.5-7.5 3.8-.6 6.9-3.7 7.5-7.5z" />
    </svg>
    <span>Orchestrator's Brain</span>
  </div>

  <button class="buscar hov" onclick={() => app.abrirBuscador()}>
    <Search size={17} />
    <span>Buscar</span>
    <kbd>Ctrl K</kbd>
  </button>

  <div class="grupo">
    <button class="item hov-fondo" class:activo={app.vista === "inicio"} onclick={() => app.irVista("inicio")}>
      <House size={17} />
      <span>Inicio</span>
    </button>
    <button class="item hov-fondo" class:activo={app.vista === "mapa"} onclick={() => app.irVista("mapa")}>
      <Orbit size={17} />
      <span>Mapa</span>
    </button>
    <button class="item hov-fondo" class:activo={app.vista === "sensibles"} onclick={() => app.irVista("sensibles")}>
      <Lock size={17} />
      <span>Sensibles</span>
      <span class="cuenta">{cuantosSensibles}</span>
    </button>
  </div>

  {#if carpetas.length > 0}
    <div class="grupo carpetas">
      <div class="rotulo cabeza">Tu bóveda</div>
      {#each carpetas as c (c.id)}
        <button
          class="item hov-fondo"
          class:activo={abiertas.has(c.id)}
          onclick={() => app.abrirCarpeta(c.id)}
          title={c.path}
        >
          <Folder size={17} />
          <span class="nombre">{nombreBonito(c.label)}</span>
          <span class="cuenta">{c.weight}</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="pie">
    <div class="estado">
      <span class="punto" style:background={estado.color}></span>
      <span class="texto">{estado.texto}</span>
      <button
        class="icono hov-fondo"
        class:gira={app.indexando}
        onclick={() => app.actualizar()}
        aria-label="Actualizar: volver a leer tus carpetas"
        title="Actualizar"
      >
        <RefreshCw size={15} />
      </button>
    </div>
    <div class="acciones">
      <button class="accion hov-fondo" onclick={() => app.anadirCarpeta()}>
        <FolderPlus size={15} />
        <span>Añadir carpeta</span>
      </button>
      <button
        class="icono hov-fondo"
        onclick={() => app.alternarTema()}
        aria-label={app.tema === "noche" ? "Cambiar al aspecto de día" : "Cambiar al aspecto de noche"}
        title={app.tema === "noche" ? "Aspecto de día" : "Aspecto de noche"}
      >
        {#if app.tema === "noche"}<Sun size={15} />{:else}<Moon size={15} />{/if}
      </button>
    </div>
    {#if ruta}
      <div class="ruta" title={ruta}>{ruta}</div>
    {/if}
  </div>
</nav>

<style>
  .lateral {
    display: flex;
    flex-direction: column;
    gap: 22px;
    width: var(--ancho-lateral);
    flex-shrink: 0;
    padding: 22px 14px 16px;
    background: var(--lateral);
    border-right: 1px solid var(--linea);
    overflow-y: auto;
  }

  .marca {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
  }
  .marca svg {
    width: 20px;
    height: 20px;
    fill: var(--calido);
  }
  .marca span {
    font-family: var(--serif);
    font-size: 22px;
    line-height: 1;
    letter-spacing: 0.2px;
  }

  .buscar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 12px;
    border-radius: 10px;
    background: var(--chip);
    border: 1px solid var(--linea);
    color: var(--tenue);
  }
  .buscar span {
    flex-grow: 1;
  }

  .grupo {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cabeza {
    padding: 0 10px 6px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 35px;
    padding: 0 10px;
    border-radius: 8px;
    color: var(--tenue);
  }
  .item.activo {
    background: var(--sel);
    color: var(--tinta);
  }
  .item span:first-of-type {
    flex-grow: 1;
  }
  .nombre {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cuenta {
    font-size: 12px;
    color: var(--tenue);
    font-variant-numeric: tabular-nums;
  }

  .pie {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 4px 0;
    border-top: 1px solid var(--linea);
  }
  .estado,
  .acciones {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .estado {
    padding-left: 6px;
  }
  .punto {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .texto {
    flex-grow: 1;
    font-size: 12.5px;
    color: var(--tenue);
  }
  .icono {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    color: var(--tenue);
  }
  .accion {
    flex-grow: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 12.5px;
    color: var(--tenue);
  }
  .ruta {
    padding: 0 6px;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--tenue);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .gira :global(svg) {
    animation: girar 1.1s linear infinite;
  }
  @keyframes girar {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .gira :global(svg) {
      animation: none;
    }
  }
</style>
