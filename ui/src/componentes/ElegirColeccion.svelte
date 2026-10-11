<!--
  El menú para meter documentos en una colección (ADR-0031), o crear una
  nueva ahí mismo.

  Desde la vista previa (un documento) cada colección sale con su marca si ya
  está en ella, y pulsarla lo mete o lo saca sin cerrar el menú. Desde la
  barra de «varios elegidos», pulsar una los mete a todos y se cierra.

  Nada se mueve de su carpeta: una colección sólo junta.
-->
<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import Layers from "@lucide/svelte/icons/layers";
  import Plus from "@lucide/svelte/icons/plus";
  import { app } from "../lib/app.svelte";

  interface Props {
    /** Los documentos que se meten. */
    docs: string[];
    /** Con un solo documento: las colecciones donde ya está. */
    dentro?: string[] | null;
    /** Hacia dónde se abre respecto a su botón. */
    hacia?: "arriba" | "abajo";
    oncerrar: () => void;
  }

  let { docs, dentro = null, hacia = "abajo", oncerrar }: Props = $props();

  let creando = $state(false);
  let nombre = $state("");
  let ocupado = $state(false);
  let campo: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (creando) campo?.focus();
  });

  // Mientras está abierto, las teclas sueltas son suyas: `Esc` lo cierra a él
  // y no a la vista previa, y escribir no abre el buscador.
  $effect(() => {
    app.menu = true;
    return () => (app.menu = false);
  });

  async function pulsar(id: string): Promise<void> {
    if (ocupado) return;
    ocupado = true;
    try {
      if (dentro?.includes(id)) await app.sacar(id, docs);
      else await app.meter(id, docs);
    } finally {
      ocupado = false;
    }
    if (dentro === null) oncerrar();
  }

  async function crear(): Promise<void> {
    if (!nombre.trim() || ocupado) return;
    ocupado = true;
    const id = await app.crearColeccion(nombre, docs);
    ocupado = false;
    if (id) oncerrar();
  }

  function tecla(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (creando) creando = false;
      else oncerrar();
    } else if (e.key === "Enter") {
      e.preventDefault();
      void crear();
    }
  }

  function cuantos(n: number): string {
    return n === 1 ? "1 documento" : `${n} documentos`;
  }
</script>

<button class="cortina" onclick={oncerrar} aria-label="Cerrar" tabindex="-1"></button>
<div class="menu entra {hacia}" role="menu" aria-label="Añadir a una colección">
  <div class="rotulo cabeza">
    {docs.length === 1 ? "En tus colecciones" : `Añadir ${docs.length} a una colección`}
  </div>
  {#if app.colecciones.length === 0 && !creando}
    <p class="pista">Todavía no tienes ninguna. Crea la primera: «Trámite de la visa», «Todo lo del piso»…</p>
  {/if}
  <div class="opciones">
    {#each app.colecciones as c (c.id)}
      {@const esta = dentro?.includes(c.id) ?? false}
      <button
        class="opcion hov-fondo"
        role="menuitemcheckbox"
        aria-checked={esta}
        disabled={ocupado}
        onclick={() => pulsar(c.id)}
        title={esta ? "Pulsa para sacarlo de esta colección" : "Pulsa para añadirlo"}
      >
        <span class="icono" class:esta>{#if esta}<Check size={14} />{:else}<Layers size={14} />{/if}</span>
        <span class="nombre">{c.name}</span>
        <span class="cuenta">{cuantos(c.documents)}</span>
      </button>
    {/each}
  </div>
  {#if creando}
    <div class="nueva">
      <input
        bind:this={campo}
        bind:value={nombre}
        onkeydown={tecla}
        class="campo"
        placeholder="Nombre de la colección"
        aria-label="Nombre de la colección nueva"
        maxlength="80"
        disabled={ocupado}
      />
      <button class="boton-prim hov" disabled={!nombre.trim() || ocupado} onclick={crear}>Crear</button>
    </div>
  {:else}
    <button class="opcion nueva-btn hov-fondo" onclick={() => (creando = true)}>
      <span class="icono"><Plus size={14} /></span>
      <span class="nombre">Nueva colección…</span>
    </button>
  {/if}
</div>

<svelte:window onkeydown={(e) => e.key === "Escape" && !creando && (e.preventDefault(), oncerrar())} />

<style>
  .cortina {
    position: fixed;
    inset: 0;
    z-index: 40;
    cursor: default;
  }
  .menu {
    position: absolute;
    left: 0;
    z-index: 41;
    width: 290px;
    max-height: 340px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--linea);
    box-shadow: 0 22px 60px -18px var(--sombra);
  }
  .menu.abajo {
    top: calc(100% + 8px);
  }
  .menu.arriba {
    bottom: calc(100% + 8px);
  }
  .cabeza {
    padding: 4px 8px 6px;
  }
  .pista {
    padding: 0 8px 6px;
    font-size: 12.5px;
    color: var(--tenue);
  }
  .opciones {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    min-height: 0;
  }
  .opcion {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    border-radius: 9px;
  }
  .opcion:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .icono {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--chip);
    color: var(--tenue);
    flex-shrink: 0;
  }
  .icono.esta {
    background: var(--acento);
    color: var(--prim-tinta);
  }
  .nombre {
    flex-grow: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }
  .cuenta {
    font-size: 12px;
    color: var(--tenue);
    white-space: nowrap;
  }
  .nueva-btn {
    color: var(--acento);
    border-top: 1px solid var(--linea);
    border-radius: 0 0 9px 9px;
    margin-top: 2px;
  }
  .nueva-btn .nombre {
    font-weight: 500;
  }
  .nueva {
    display: flex;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--linea);
  }
  .nueva .campo {
    flex-grow: 1;
    min-width: 0;
    height: 40px;
    font-size: 14px;
  }
</style>
