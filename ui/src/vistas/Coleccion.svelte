<!--
  Una colección abierta (ADR-0031): se ve como una carpeta —cuadrícula o
  lista, un clic la vista previa, dos clics lo abre—, pero cada documento dice
  **su carpeta de verdad**, porque vienen de sitios distintos.

  Lo que está dentro de un paquete que también metiste va justo debajo de él
  (ADR-0030). Lo que está en la papelera no sale en la lista, pero se dice
  abajo, y si lo restauras vuelve solo (ADR-0025).

  Renombrar es un clic; borrar la colección pide dos, y nunca toca tus
  documentos.
-->
<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import Cloud from "@lucide/svelte/icons/cloud";
  import CornerDownRight from "@lucide/svelte/icons/corner-down-right";
  import Folder from "@lucide/svelte/icons/folder";
  import Lock from "@lucide/svelte/icons/lock";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Miniatura from "../componentes/Miniatura.svelte";
  import Tipo from "../componentes/Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import { abrir } from "../lib/nucleo";
  import type { Miembro } from "../lib/nucleo";
  import { fecha, nombreBonito, tipoDe, tipoLargo, tituloDe } from "../lib/nombres";

  const c = $derived(app.abierta);
  const orden = $derived((c?.rows ?? []).map((r) => r.id));
  const nombres = $derived(new Map((c?.rows ?? []).map((r) => [r.id, tituloDe(r.label).titulo])));

  let renombrando = $state(false);
  let nombre = $state("");
  let campo: HTMLInputElement | undefined = $state();
  let confirmando = $state(false);
  let olvido: number | undefined;

  $effect(() => {
    void c?.id;
    renombrando = false;
    confirmando = false;
  });

  $effect(() => {
    if (renombrando) campo?.select();
  });

  const resumen = $derived.by(() => {
    if (!c) return "";
    const n = c.rows.length;
    if (n === 0) return c.in_trash.length ? "Nada en tu bóveda" : "Vacía";
    const docs = n === 1 ? "1 documento" : `${n} documentos`;
    const carpetas = c.folders === 1 ? "1 carpeta" : `${c.folders} carpetas`;
    return `${docs} de ${carpetas}`;
  });

  function ficha(f: Miembro) {
    const t = tituloDe(f.label);
    const tipo = tipoDe(f.label);
    return {
      titulo: t.titulo,
      cuando: t.fecha ?? fecha(f.recency),
      tipo,
      largo: tipoLargo(tipo),
      sensible: f.sensitivity !== "normal",
      nube: f.availability !== null && f.availability !== "local",
      paquete: f.inside ? (nombres.get(f.inside) ?? "") : "",
    };
  }

  function abrirArchivo(f: Miembro): void {
    abrir(f.id).catch((e) => app.avisar(`No se pudo abrir — ${e}`));
  }

  function empezarRenombrar(): void {
    if (!c) return;
    nombre = c.name;
    renombrando = true;
  }

  async function guardarNombre(): Promise<void> {
    if (!c || !nombre.trim()) return;
    if (nombre.trim() === c.name) {
      renombrando = false;
      return;
    }
    if (await app.renombrarColeccion(c.id, nombre)) renombrando = false;
  }

  function tecla(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      void guardarNombre();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      renombrando = false;
    }
  }

  function borrar(): void {
    if (!c) return;
    if (!confirmando) {
      confirmando = true;
      window.clearTimeout(olvido);
      olvido = window.setTimeout(() => (confirmando = false), 4000);
      return;
    }
    confirmando = false;
    void app.borrarColeccion(c.id);
  }

  const elegida = (id: string) => app.seleccion === id || app.elegidos.includes(id);
</script>

{#if c}
  <div class="coleccion entra">
    <div class="titulo">
      {#if renombrando}
        <div class="renombrar">
          <input
            bind:this={campo}
            bind:value={nombre}
            onkeydown={tecla}
            class="campo"
            aria-label="Nombre de la colección"
            maxlength="80"
          />
          <button class="boton-prim hov" disabled={!nombre.trim()} onclick={guardarNombre}><Check size={15} /> Guardar</button>
          <button class="boton-sec hov" onclick={() => (renombrando = false)}>Cancelar</button>
        </div>
      {:else}
        <h1 class="titular">{c.name}</h1>
      {/if}
      <div class="sub">
        <p>{resumen}</p>
        {#if !renombrando}
          <button class="enlace hov-fondo" onclick={empezarRenombrar}><Pencil size={13} /> Renombrar</button>
          <button
            class="enlace borrar hov-fondo"
            class:confirmando
            onclick={borrar}
            title={confirmando ? "Pulsa otra vez: se borra la colección, no tus documentos" : "Borrar la colección. Tus documentos no se tocan."}
          >
            <Trash2 size={13} />
            {confirmando ? "¿Borrar la colección? Tus documentos no se tocan" : "Borrar colección"}
          </button>
        {/if}
      </div>
    </div>

    {#if c.rows.length === 0}
      <p class="vacio">
        {c.in_trash.length
          ? "Lo que tenía está en la papelera."
          : "Todavía no tiene nada. Abre un documento y pulsa «Colecciones» en su vista previa, o elige varios en una carpeta con Ctrl+clic."}
      </p>
    {:else if app.modo === "cuadricula"}
      <div class="cuadricula">
        {#each c.rows as f (f.id)}
          {@const d = ficha(f)}
          <button
            class="tarjeta hov"
            class:elegida={elegida(f.id)}
            aria-pressed={elegida(f.id)}
            onclick={(e) => app.pulsar(f.id, e, orden)}
            ondblclick={() => abrirArchivo(f)}
            title={f.label}
          >
            <Miniatura fila={f} />
            <span class="texto">
              <span class="nombre-doc">{d.titulo}</span>
              <span class="meta">{d.largo}{d.cuando ? ` · ${d.cuando}` : ""}</span>
              <span class="marcas">
                <span class="marca"><Folder size={11} /> <span class="corta">{nombreBonito(f.folder_label)}</span></span>
                {#if d.paquete}<span class="marca" title="Está dentro de «{d.paquete}»"><CornerDownRight size={11} /> <span class="corta">dentro de «{d.paquete}»</span></span>{/if}
                {#if d.sensible}<span class="marca"><Lock size={11} /> Sensible</span>{/if}
                {#if d.nube}<span class="marca"><Cloud size={11} /> En la nube</span>{/if}
              </span>
            </span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="lista">
        <div class="fila encabezado rotulo">
          <span>Nombre</span><span>Carpeta</span><span>Tipo</span><span>Fecha</span><span></span>
        </div>
        {#each c.rows as f (f.id)}
          {@const d = ficha(f)}
          <button
            class="fila hov-fondo"
            class:elegida={elegida(f.id)}
            class:metida={!!f.inside}
            aria-pressed={elegida(f.id)}
            onclick={(e) => app.pulsar(f.id, e, orden)}
            ondblclick={() => abrirArchivo(f)}
            title={d.paquete ? `${f.label} · está dentro de «${d.paquete}»` : f.label}
          >
            <span class="primera">
              {#if f.inside}<CornerDownRight size={14} class="metido" />{/if}
              <Tipo tipo={d.tipo} />
              <span class="nombre-doc">{d.titulo}</span>
            </span>
            <span class="tenue">{nombreBonito(f.folder_label)}</span>
            <span class="tenue">{d.largo}</span>
            <span class="tenue">{d.cuando}</span>
            <span class="iconos">
              {#if d.sensible}<Lock size={14} aria-label="Sensible" />{/if}
              {#if d.nube}<Cloud size={14} aria-label="En la nube" />{/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}

    {#if c.in_trash.length > 0}
      <section class="papelera" aria-label="En la papelera">
        <h2 class="rotulo">En la papelera</h2>
        <p class="pista">Ya no están en tu bóveda. Si los sacas de la Papelera de Windows, vuelven solos a esta colección.</p>
        <div class="fuera">
          {#each c.in_trash as p (p.id)}
            <button class="fila-fuera hov-fondo" class:elegida={app.seleccion === p.id} onclick={() => app.ver(p.id)}>
              <Trash2 size={14} />
              <span class="nombre-doc">{tituloDe(p.label).titulo}</span>
              <span class="tenue">
                {p.days_left <= 0 ? "se olvida hoy" : `se olvida en ${p.days_left === 1 ? "1 día" : `${p.days_left} días`}`}
              </span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  </div>
{/if}

<style>
  .coleccion {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 32px 36px 90px;
  }

  .titulo {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .titulo h1 {
    font-size: 42px;
    overflow-wrap: anywhere;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .sub p {
    color: var(--tenue);
    margin-right: 8px;
  }
  .enlace {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: 7px;
    font-size: 12.5px;
    color: var(--tenue);
  }
  .enlace.borrar.confirmando {
    color: var(--peligro);
    background: var(--peligro-fondo);
  }
  .renombrar {
    display: flex;
    gap: 8px;
    align-items: center;
    max-width: 640px;
  }
  .renombrar .campo {
    flex-grow: 1;
    min-width: 0;
    font-family: var(--serif);
    font-size: 30px;
    height: 54px;
  }
  .vacio,
  .pista {
    color: var(--tenue);
    max-width: 680px;
  }
  .pista {
    font-size: 13px;
  }

  .cuadricula {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 16px;
  }
  .tarjeta {
    display: flex;
    flex-direction: column;
    border-radius: 14px;
    overflow: hidden;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  .tarjeta.elegida {
    border-color: var(--acento);
    box-shadow: 0 0 0 1px var(--acento);
  }
  .texto {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px 14px;
  }
  .nombre-doc {
    font-size: 14.5px;
    font-weight: 500;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .meta,
  .tenue {
    font-size: 12.5px;
    color: var(--tenue);
  }
  .marcas {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .marca {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--chip);
    color: var(--tenue);
    font-size: 11.5px;
    font-weight: 500;
    max-width: 100%;
    white-space: nowrap;
  }
  .marca :global(svg) {
    flex-shrink: 0;
  }
  .corta {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .lista {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--linea);
    border-radius: 14px;
    overflow: hidden;
    background: var(--tarjeta);
  }
  .fila {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px 120px 110px 50px;
    gap: 12px;
    align-items: center;
    padding: 11px 18px;
    border-top: 1px solid var(--linea);
  }
  .fila.encabezado {
    border-top: 0;
    padding-block: 10px;
  }
  .fila.elegida {
    background: var(--sel);
  }
  .fila .tenue {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .primera {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .fila.metida .primera {
    padding-left: 10px;
  }
  .primera :global(.metido) {
    color: var(--tenue);
    flex-shrink: 0;
    margin-right: -4px;
  }
  .primera .nombre-doc {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .iconos {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    color: var(--tenue);
  }

  .papelera {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .fuera {
    display: flex;
    flex-direction: column;
    border: 1px dashed var(--linea);
    border-radius: 12px;
    overflow: hidden;
  }
  .fila-fuera {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    padding: 10px 16px;
    color: var(--tenue);
  }
  .fila-fuera + .fila-fuera {
    border-top: 1px dashed var(--linea);
  }
  .fila-fuera.elegida {
    background: var(--sel);
  }
  .fila-fuera .nombre-doc {
    color: var(--tinta);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
