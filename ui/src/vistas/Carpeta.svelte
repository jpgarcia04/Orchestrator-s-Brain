<!--
  Una carpeta de la bóveda, como la enseña un sistema operativo: sus carpetas
  arriba, sus documentos debajo, en cuadrícula o en lista.

  Un clic enseña el documento en la vista previa; dos clics lo abren con su
  programa, igual que en el explorador de Windows. Se entra en una carpeta con
  un clic, porque en una carpeta no hay nada que previsualizar.
-->
<script lang="ts">
  import Cloud from "@lucide/svelte/icons/cloud";
  import Folder from "@lucide/svelte/icons/folder";
  import Lock from "@lucide/svelte/icons/lock";
  import Miniatura from "../componentes/Miniatura.svelte";
  import Tipo from "../componentes/Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import { abrir } from "../lib/nucleo";
  import type { Fila } from "../lib/nucleo";
  import { fecha, nombreBonito, tipoDe, tipoLargo, tituloDe } from "../lib/nombres";

  const listado = $derived(app.listado);
  const carpetas = $derived((listado?.rows ?? []).filter((r) => r.kind === "folder"));
  const archivos = $derived((listado?.rows ?? []).filter((r) => r.kind === "file"));

  const resumen = $derived.by(() => {
    const partes: string[] = [];
    if (carpetas.length) partes.push(carpetas.length === 1 ? "1 carpeta" : `${carpetas.length} carpetas`);
    partes.push(archivos.length === 1 ? "1 documento" : `${archivos.length} documentos`);
    return partes.join(" · ");
  });

  /** Lo que se lee de cada archivo, calculado una vez por fila. */
  function ficha(f: Fila) {
    const t = tituloDe(f.label);
    const tipo = tipoDe(f.label);
    return {
      titulo: t.titulo,
      cuando: t.fecha ?? fecha(f.recency),
      tipo,
      largo: tipoLargo(tipo),
      sensible: f.sensitivity !== "normal",
      nube: f.availability !== null && f.availability !== "local",
    };
  }

  function abrirArchivo(f: Fila): void {
    abrir(f.id).catch((e) => app.avisar(`No se pudo abrir — ${e}`));
  }
</script>

{#if listado}
  <div class="carpeta entra">
    <div class="titulo">
      <h1 class="titular">{nombreBonito(listado.label)}</h1>
      <p>{resumen}</p>
    </div>

    {#if carpetas.length > 0}
      <div class="subcarpetas">
        {#each carpetas as c (c.id)}
          <button class="subcarpeta hov" onclick={() => app.abrirCarpeta(c.id)} title={c.path}>
            <Folder size={20} />
            <span class="nombre">{nombreBonito(c.label)}</span>
            <span class="cuenta">{c.weight}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if archivos.length === 0}
      <p class="vacio">
        {carpetas.length ? "Aquí sólo hay carpetas." : "Esta carpeta está vacía, o lo que hay dentro no se indexa."}
      </p>
    {:else if app.modo === "cuadricula"}
      <div class="cuadricula">
        {#each archivos as f (f.id)}
          {@const d = ficha(f)}
          <button
            class="tarjeta hov"
            class:elegida={app.seleccion === f.id}
            aria-pressed={app.seleccion === f.id}
            onclick={() => app.ver(f.id)}
            ondblclick={() => abrirArchivo(f)}
            title={f.label}
          >
            <Miniatura fila={f} />
            <span class="texto">
              <span class="nombre-doc">{d.titulo}</span>
              <span class="meta">{d.largo}{d.cuando ? ` · ${d.cuando}` : ""}</span>
              {#if d.sensible || d.nube}
                <span class="marcas">
                  {#if d.sensible}<span class="marca"><Lock size={11} /> Sensible</span>{/if}
                  {#if d.nube}<span class="marca"><Cloud size={11} /> En la nube</span>{/if}
                </span>
              {/if}
            </span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="lista">
        <div class="fila encabezado rotulo">
          <span>Nombre</span><span>Tipo</span><span>Fecha</span><span></span>
        </div>
        {#each archivos as f (f.id)}
          {@const d = ficha(f)}
          <button
            class="fila hov-fondo"
            class:elegida={app.seleccion === f.id}
            aria-pressed={app.seleccion === f.id}
            onclick={() => app.ver(f.id)}
            ondblclick={() => abrirArchivo(f)}
            title={f.label}
          >
            <span class="primera">
              <Tipo tipo={d.tipo} />
              <span class="nombre-doc">{d.titulo}</span>
            </span>
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
  </div>
{/if}

<style>
  .carpeta {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 32px 36px 44px;
  }

  .titulo {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .titulo h1 {
    font-size: 42px;
  }
  .titulo p,
  .vacio {
    color: var(--tenue);
  }

  .subcarpetas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px;
  }
  .subcarpeta {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 48px;
    padding: 0 14px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    color: var(--tenue);
  }
  .subcarpeta .nombre {
    flex-grow: 1;
    min-width: 0;
    color: var(--tinta);
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cuenta {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
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
    grid-template-columns: minmax(0, 1fr) 130px 120px 50px;
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
  .primera {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
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
</style>
