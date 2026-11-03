<!--
  Una carpeta de la bóveda, como la enseña un sistema operativo: sus carpetas
  arriba, sus documentos debajo, en cuadrícula o en lista.

  Un clic enseña el documento en la vista previa; dos clics lo abren con su
  programa, igual que en el explorador de Windows. Se entra en una carpeta con
  un clic, porque en una carpeta no hay nada que previsualizar.

  Ctrl+clic y Mayús+clic eligen varios, y Ctrl+A todos: abajo sale la barra
  para meterlos de golpe en una colección (ADR-0031). O se arrastran a una
  colección de la barra lateral (ADR-0033).
-->
<script lang="ts">
  import Cloud from "@lucide/svelte/icons/cloud";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderMinus from "@lucide/svelte/icons/folder-minus";
  import Modal from "../componentes/Modal.svelte";
  import Lock from "@lucide/svelte/icons/lock";
  import Miniatura from "../componentes/Miniatura.svelte";
  import Tipo from "../componentes/Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import { arrastrable } from "../lib/arrastre";
  import { abrir } from "../lib/nucleo";
  import type { Fila } from "../lib/nucleo";
  import { fecha, nombreBonito, tipoDe, tipoLargo, tituloDe } from "../lib/nombres";

  const listado = $derived(app.listado);
  const carpetas = $derived((listado?.rows ?? []).filter((r) => r.kind === "folder"));
  const archivos = $derived((listado?.rows ?? []).filter((r) => r.kind === "file"));
  const orden = $derived(archivos.map((f) => f.id));
  const elegida = (id: string) => app.seleccion === id || app.elegidos.includes(id);
  /** Lo que va en el arrastre en curso se atenúa (ADR-0033). */
  const llevado = (id: string) => !!app.arrastre?.docs.includes(id);

  const esBiblioteca = $derived.by(() => {
    if (!listado?.path) return false;
    const p = listado.path.replace(/[\\/]+$/, "").toLowerCase();
    return (app.info?.libraries ?? []).some(
      (lib) => lib.replace(/[\\/]+$/, "").toLowerCase() === p
    );
  });

  let modalRetiro = $state(false);
  let retirando = $state(false);

  async function confirmarRetiro(): Promise<void> {
    if (!listado?.path || retirando) return;
    retirando = true;
    const ruta = listado.path;
    const nombre = nombreBonito(listado.label);
    const inf = await app.retirarCarpeta(ruta);
    retirando = false;
    modalRetiro = false;
    if (inf) {
      app.avisar(`Biblioteca «${nombre}» retirada de tu bóveda (${inf.documentos_retirados} documentos desvinculados).`);
    }
  }

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
    <div class="cabecera-carpeta">
      <div class="titulo">
        <h1 class="titular">{nombreBonito(listado.label)}</h1>
        <p>{resumen}</p>
      </div>
      {#if esBiblioteca}
        <button
          class="boton-retirar hov"
          onclick={() => (modalRetiro = true)}
          title="Retirar esta biblioteca de tu bóveda"
        >
          <FolderMinus size={15} />
          <span>Retirar biblioteca</span>
        </button>
      {/if}
    </div>

    {#if carpetas.length > 0}
      <div class="subcarpetas">
        {#each carpetas as c (c.id)}
          <button class="subcarpeta hov" onclick={() => app.abrirCarpeta(c.id)} title={c.path}>
            <Folder size={20} />
            <span class="nombre">{nombreBonito(c.label)}</span>
            {#if c.sensitivity !== "normal"}<span class="candado"><Lock size={13} aria-label="Sensible" /></span>{/if}
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
            class:elegida={elegida(f.id)}
            class:llevado={llevado(f.id)}
            aria-pressed={elegida(f.id)}
            onclick={(e) => app.pulsar(f.id, e, orden)}
            use:arrastrable={{ id: f.id, titulo: d.titulo }}
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
            class:elegida={elegida(f.id)}
            class:llevado={llevado(f.id)}
            aria-pressed={elegida(f.id)}
            onclick={(e) => app.pulsar(f.id, e, orden)}
            use:arrastrable={{ id: f.id, titulo: d.titulo }}
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

{#if modalRetiro && listado}
  <Modal titulo="Retirar biblioteca" cerrar={() => (modalRetiro = false)}>
    <div class="modal-retiro">
      <p class="pregunta">
        ¿Quieres retirar <strong>«{nombreBonito(listado.label)}»</strong> de tu bóveda?
      </p>
      <div class="ruta-destacada">{listado.path}</div>

      <ul class="advertencias">
        <li>
          <strong>Tus archivos en Windows no se tocan:</strong> Los archivos originales de tu equipo no se borrarán ni modificarán en ningún momento.
        </li>
        <li>
          <strong>Dejar de vigilar e indexar:</strong> Brain dejará de observar esta carpeta en segundo plano y no volverá a sincronizarla.
        </li>
        <li>
          <strong>Limpieza del índice:</strong> Los documentos, fragmentos de búsqueda y vectores asociados exclusivamente a esta carpeta se retirarán de tu base de conocimiento.
        </li>
      </ul>

      <div class="acciones-modal">
        <button class="boton-sec hov" onclick={() => (modalRetiro = false)} disabled={retirando}>
          Cancelar
        </button>
        <button class="boton-peligro hov" onclick={confirmarRetiro} disabled={retirando}>
          {retirando ? "Retirando…" : "Retirar biblioteca"}
        </button>
      </div>
    </div>
  </Modal>
{/if}

<style>
  .carpeta {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 32px 36px 90px;
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
  .candado {
    display: grid;
    color: var(--calido);
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
  /* Lo que llevas arrastrando se queda atenuado en su sitio (ADR-0033). */
  .llevado {
    opacity: 0.45;
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
  .cabecera-carpeta {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .boton-retirar {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 8px;
    background: transparent;
    border: 1px solid var(--linea);
    color: var(--tenue);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    margin-top: 4px;
  }

  .boton-retirar:hover {
    color: var(--peligro);
    border-color: var(--peligro);
    background: rgba(238, 85, 85, 0.08);
  }

  .modal-retiro {
    display: flex;
    flex-direction: column;
    gap: 16px;
    font-size: 13.5px;
    line-height: 1.5;
  }

  .modal-retiro .pregunta {
    margin: 0;
    color: var(--texto);
  }

  .ruta-destacada {
    font-family: var(--mono, monospace);
    font-size: 12px;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--chip);
    border: 1px solid var(--linea);
    word-break: break-all;
    color: var(--texto);
  }

  .advertencias {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    color: var(--tenue);
  }

  .advertencias li strong {
    color: var(--texto);
  }

  .acciones-modal {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 8px;
  }

  .boton-sec {
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    color: var(--texto);
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
  }

  .boton-peligro {
    background: var(--peligro);
    color: white;
    border: none;
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }

  .boton-peligro:hover:not(:disabled) {
    opacity: 0.9;
  }

  .boton-peligro:disabled,
  .boton-sec:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
