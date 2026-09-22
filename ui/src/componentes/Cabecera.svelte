<!--
  La cabecera: dónde estás, en migas cortas —`Bóveda › Salud › Pólizas`—, y
  dentro de una carpeta, cómo verla.

  Las migas empiezan en tu bóveda, no en la raíz del disco (ADR-0018). Si el
  camino es largo, los escalones del medio se pliegan en «…» y el último nunca
  se recorta.
-->
<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import List from "@lucide/svelte/icons/list";
  import Orbit from "@lucide/svelte/icons/orbit";
  import { app } from "../lib/app.svelte";
  import { nombreBonito } from "../lib/nombres";

  interface Miga {
    texto: string;
    ir: () => void;
  }

  const NOMBRES = { inicio: "Inicio", mapa: "Mapa", sensibles: "Sensibles" } as const;

  const migas = $derived.by((): Miga[] => {
    if (app.vista !== "carpeta" || !app.listado) {
      const v = app.vista === "carpeta" ? "inicio" : app.vista;
      return [{ texto: NOMBRES[v], ir: () => app.irVista(v) }];
    }
    const l = app.listado;
    const camino: Miga[] = l.trail.map((m) => ({ texto: nombreBonito(m.label), ir: () => app.abrirCarpeta(m.id) }));
    camino.push({ texto: nombreBonito(l.label), ir: () => app.cerrarPanel() });
    return camino;
  });

  /** Más de cuatro escalones: el primero, «…» y los dos últimos. */
  const visibles = $derived.by(() => {
    if (migas.length <= 4) return migas.map((m) => ({ ...m, pliegue: false }));
    const primera = migas[0]!;
    const medio = migas[migas.length - 3]!;
    return [
      { ...primera, pliegue: false },
      { texto: "…", ir: medio.ir, pliegue: true },
      ...migas.slice(-2).map((m) => ({ ...m, pliegue: false })),
    ];
  });
</script>

<header class="cabecera">
  <nav aria-label="Dónde estás" class="migas">
    {#each visibles as m, i (i)}
      {#if i > 0}<ChevronRight size={14} class="sep" />{/if}
      <button
        class="miga hov-fondo"
        class:ultima={i === visibles.length - 1}
        onclick={m.ir}
        title={m.pliegue ? "Subir" : m.texto}
      >
        {m.texto}
      </button>
    {/each}
  </nav>

  {#if app.vista === "carpeta" && app.listado?.focus}
    {@const foco = app.listado.focus}
    <button class="ver-mapa hov-fondo" onclick={() => app.verEnMapa(foco)}>
      <Orbit size={16} />
      <span>Ver en el mapa</span>
    </button>
    <div class="modos" role="group" aria-label="Cómo ver la carpeta">
      <button
        class:activo={app.modo === "cuadricula"}
        onclick={() => app.ponerModo("cuadricula")}
        aria-label="Cuadrícula"
        aria-pressed={app.modo === "cuadricula"}
        title="Cuadrícula"
      >
        <LayoutGrid size={16} />
      </button>
      <button
        class:activo={app.modo === "lista"}
        onclick={() => app.ponerModo("lista")}
        aria-label="Lista"
        aria-pressed={app.modo === "lista"}
        title="Lista"
      >
        <List size={16} />
      </button>
    </div>
  {/if}
</header>

<style>
  .cabecera {
    display: flex;
    align-items: center;
    gap: 12px;
    height: var(--alto-cabecera);
    flex-shrink: 0;
    padding: 0 24px 0 26px;
    border-bottom: 1px solid var(--linea);
  }

  .migas {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-grow: 1;
    min-width: 0;
  }
  .migas :global(.sep) {
    flex-shrink: 0;
    color: var(--tenue);
  }
  .miga {
    padding: 5px 8px;
    border-radius: 7px;
    color: var(--tenue);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .miga.ultima {
    color: var(--tinta);
    font-weight: 500;
    flex-shrink: 0;
  }

  .ver-mapa {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-radius: 9px;
    color: var(--tenue);
  }

  .modos {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 10px;
    background: var(--chip);
    border: 1px solid var(--linea);
  }
  .modos button {
    display: grid;
    place-items: center;
    width: 34px;
    height: 28px;
    border-radius: 7px;
    color: var(--tenue);
  }
  .modos button.activo {
    background: var(--tarjeta);
    color: var(--tinta);
  }
</style>
