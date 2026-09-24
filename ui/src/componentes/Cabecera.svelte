<!--
  La cabecera: dónde estás, en migas cortas —`Bóveda › Salud › Pólizas`—, y
  dentro de una carpeta, cómo verla.

  Las migas empiezan en tu bóveda, no en la raíz del disco (ADR-0018). Si el
  camino es largo, los escalones del medio se pliegan en «…» y el último nunca
  se recorta.

  Dentro de una carpeta, el escudo la marca como sensible: todo lo que tenga
  dentro lo hereda, también lo que llegue después (ADR-0020). Marcar es un
  clic; quitar la marca baja la protección y por eso pide un segundo clic. Si
  la marca es de una carpeta de fuera, el botón lo dice y lleva hasta ella.

  Dentro de una colección (ADR-0031): buscar sólo en ella, verla en el mapa
  —sus estrellas encendidas— y cómo verla. Una colección no es sensible: no
  tiene escudo.
-->
<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import List from "@lucide/svelte/icons/list";
  import Orbit from "@lucide/svelte/icons/orbit";
  import Search from "@lucide/svelte/icons/search";
  import Shield from "@lucide/svelte/icons/shield";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import { app } from "../lib/app.svelte";
  import { nombreBonito } from "../lib/nombres";

  /** El segundo clic para quitar la marca: dura unos segundos y se olvida. */
  let confirmando = $state(false);
  let olvido: number | undefined;

  $effect(() => {
    void app.listado?.focus;
    confirmando = false;
  });

  function quitarMarca(id: string, nombre: string): void {
    if (!confirmando) {
      confirmando = true;
      window.clearTimeout(olvido);
      olvido = window.setTimeout(() => (confirmando = false), 4000);
      return;
    }
    confirmando = false;
    void app.marcarCarpeta(id, nombre, null);
  }

  interface Miga {
    texto: string;
    ir: () => void;
  }

  const NOMBRES = {
    inicio: "Inicio",
    mapa: "Mapa",
    sensibles: "Sensibles",
    papelera: "Papelera",
    colecciones: "Colecciones",
  } as const;

  const migas = $derived.by((): Miga[] => {
    if (app.vista === "coleccion") {
      const a = app.abierta;
      return [
        { texto: "Colecciones", ir: () => app.irVista("colecciones") },
        ...(a ? [{ texto: a.name, ir: () => app.cerrarPanel() }] : []),
      ];
    }
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
    {@const nombre = nombreBonito(app.listado.label)}
    {@const marca = app.listado.protection}
    {#if marca?.own}
      <button
        class="escudo marcada hov"
        class:confirmando
        onclick={() => quitarMarca(foco, nombre)}
        aria-pressed="true"
        title={confirmando
          ? "Pulsa otra vez para quitar la marca"
          : "Todo lo de esta carpeta es sensible, también lo que llegue después. Pulsa para quitar la marca."}
      >
        <ShieldCheck size={16} />
        <span>{confirmando ? "¿Quitar la marca?" : "Sensible"}</span>
      </button>
    {:else if marca}
      {@const madre = marca.folder}
      <button
        class="escudo heredada hov-fondo"
        onclick={() => app.abrirCarpeta(madre)}
        title="La marca está en «{nombreBonito(marca.label)}». Ábrela para cambiarla."
      >
        <ShieldCheck size={16} />
        <span>Sensible por «{nombreBonito(marca.label)}»</span>
      </button>
    {:else}
      <button
        class="escudo hov-fondo"
        onclick={() => app.marcarCarpeta(foco, nombre, "sensitive")}
        aria-pressed="false"
        title="Todo lo que tenga dentro —y lo que llegue después— será sensible: borroso hasta que lo pidas y fuera de cualquier modelo remoto."
      >
        <Shield size={16} />
        <span>Marcar como sensible</span>
      </button>
    {/if}
    <button class="ver-mapa hov-fondo" onclick={() => app.verEnMapa(foco)}>
      <Orbit size={16} />
      <span>Ver en el mapa</span>
    </button>
  {:else if app.vista === "coleccion" && app.abierta}
    {@const cid = app.abierta.id}
    <button class="ver-mapa hov-fondo" onclick={() => app.abrirBuscador()} title="Buscar sólo en esta colección">
      <Search size={16} />
      <span>Buscar aquí</span>
    </button>
    <button class="ver-mapa hov-fondo" onclick={() => app.verColeccionEnMapa(cid)} title="Sus estrellas encendidas, sin mover ninguna">
      <Orbit size={16} />
      <span>Ver en el mapa</span>
    </button>
  {/if}
  {#if (app.vista === "carpeta" && app.listado?.focus) || (app.vista === "coleccion" && app.abierta)}
    <div class="modos" role="group" aria-label="Cómo ver la lista">
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

  .ver-mapa,
  .escudo {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-radius: 9px;
    color: var(--tenue);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Marcada: el mismo cálido que avisa de lo sensible, para que se lea de un
     vistazo que esta carpeta protege lo que tiene dentro. */
  .escudo.marcada {
    color: var(--calido);
    background: var(--calido-fondo);
    font-weight: 500;
  }
  .escudo.marcada.confirmando {
    color: var(--peligro);
    background: var(--peligro-fondo);
  }
  .escudo.heredada {
    color: var(--calido);
    max-width: 280px;
  }
  .escudo.heredada span {
    overflow: hidden;
    text-overflow: ellipsis;
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
