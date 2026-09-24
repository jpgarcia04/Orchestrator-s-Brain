<!--
  Tus colecciones (ADR-0031): lo que juntas a mano —«Trámite de la visa»,
  «Todo lo del piso»— venga de la carpeta que venga. Un documento puede estar
  en varias, y ninguno se mueve de su sitio.

  Cada colección dice cuántos documentos tiene, cuántos de ellos están en la
  papelera y desde cuándo existe. Un clic la abre.
-->
<script lang="ts">
  import Layers from "@lucide/svelte/icons/layers";
  import Plus from "@lucide/svelte/icons/plus";
  import { app } from "../lib/app.svelte";
  import { fecha } from "../lib/nombres";

  let creando = $state(false);
  let nombre = $state("");
  let ocupado = $state(false);
  let campo: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (creando) campo?.focus();
  });

  async function crear(): Promise<void> {
    if (!nombre.trim() || ocupado) return;
    ocupado = true;
    const id = await app.crearColeccion(nombre);
    ocupado = false;
    if (id) {
      creando = false;
      nombre = "";
      await app.abrirColeccion(id);
    }
  }

  function tecla(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      void crear();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      creando = false;
    }
  }

  function resumen(documentos: number, papelera: number): string {
    const d = documentos === 0 ? "Vacía" : documentos === 1 ? "1 documento" : `${documentos} documentos`;
    return papelera > 0 ? `${d} · ${papelera} en la papelera` : d;
  }
</script>

<div class="colecciones entra">
  <div class="titulo">
    <h1 class="titular">Colecciones</h1>
    <p>
      Junta documentos de carpetas distintas —«Trámite de la visa», «Todo lo del piso»— sin moverlos de su sitio.
      Un documento puede estar en varias.
    </p>
  </div>

  <div class="rejilla">
    {#each app.colecciones as c (c.id)}
      <button class="coleccion hov" onclick={() => app.abrirColeccion(c.id)}>
        <span class="icono"><Layers size={18} /></span>
        <span class="texto">
          <span class="nombre">{c.name}</span>
          <span class="cuenta" class:aviso={c.in_trash > 0}>{resumen(c.documents, c.in_trash)}</span>
          <span class="desde">desde el {fecha(c.since)}</span>
        </span>
      </button>
    {/each}

    {#if creando}
      <div class="coleccion nueva">
        <input
          bind:this={campo}
          bind:value={nombre}
          onkeydown={tecla}
          class="campo"
          placeholder="Nombre: «Trámite de la visa»"
          aria-label="Nombre de la colección nueva"
          maxlength="80"
          disabled={ocupado}
        />
        <div class="botones">
          <button class="boton-prim hov" disabled={!nombre.trim() || ocupado} onclick={crear}>Crear</button>
          <button class="boton-sec hov" onclick={() => (creando = false)}>Cancelar</button>
        </div>
      </div>
    {:else}
      <button class="coleccion crear hov-fondo" onclick={() => (creando = true)}>
        <span class="icono"><Plus size={18} /></span>
        <span class="nombre">Nueva colección</span>
      </button>
    {/if}
  </div>

  {#if app.colecciones.length === 0}
    <p class="pista">
      Para meter documentos: ábrelo y pulsa <strong>«Colecciones»</strong> en su vista previa, o elige varios en una
      carpeta con <kbd>Ctrl</kbd>+clic (o <kbd>Ctrl</kbd> <kbd>A</kbd> para todos) y pulsa «Añadir a una colección».
    </p>
  {/if}
</div>

<style>
  .colecciones {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 32px 36px 44px;
  }
  .titulo {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 680px;
  }
  .titulo h1 {
    font-size: 42px;
  }
  .titulo p,
  .pista {
    color: var(--tenue);
  }
  .pista {
    max-width: 680px;
    font-size: 13px;
    line-height: 1.7;
  }
  .pista strong {
    color: var(--tinta);
    font-weight: 500;
  }

  .rejilla {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 12px;
  }
  .coleccion {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    min-height: 92px;
    padding: 16px;
    border-radius: 14px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  .icono {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--sel);
    color: var(--acento);
    flex-shrink: 0;
  }
  .texto {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .nombre {
    font-size: 15.5px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .cuenta,
  .desde {
    font-size: 12.5px;
    color: var(--tenue);
  }
  .cuenta.aviso {
    color: var(--calido);
  }
  .desde {
    font-size: 11.5px;
  }
  .crear {
    align-items: center;
    background: transparent;
    border-style: dashed;
    color: var(--acento);
  }
  .crear .icono {
    background: transparent;
    border: 1px dashed var(--linea);
  }
  .nueva {
    flex-direction: column;
    gap: 10px;
  }
  .nueva .campo {
    width: 100%;
    height: 40px;
    font-size: 14px;
  }
  .botones {
    display: flex;
    gap: 8px;
  }
  .botones button {
    height: 34px;
    padding: 0 12px;
    font-size: 13px;
  }
</style>
