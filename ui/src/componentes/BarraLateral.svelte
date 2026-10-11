<!--
  La barra lateral: por dónde se entra a todo.

  Arriba, buscar. Después las vistas —el inicio, el mapa, tus colecciones, lo
  sensible— y tu bóveda carpeta por carpeta, con su nombre limpio y cuántos
  documentos tiene; debajo, cada una de tus colecciones (ADR-0031), donde se
  sueltan los documentos que arrastras (ADR-0033).
  Abajo, el estado: si está al día, actualizar, añadir otra carpeta y el
  aspecto.
-->
<script lang="ts">
  import Folder from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import House from "@lucide/svelte/icons/house";
  import Layers from "@lucide/svelte/icons/layers";
  import LayersPlus from "@lucide/svelte/icons/layers-plus";
  import Lock from "@lucide/svelte/icons/lock";
  import Moon from "@lucide/svelte/icons/moon";
  import Orbit from "@lucide/svelte/icons/orbit";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Sun from "@lucide/svelte/icons/sun";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import { app } from "../lib/app.svelte";
  import { nombreBonito } from "../lib/nombres";
  import { papelera, sensibles } from "../lib/nucleo";

  let cuantosSensibles = $state(0);
  let enPapelera = $state(0);

  $effect(() => {
    void app.version;
    sensibles()
      .then((s) => (cuantosSensibles = s.length))
      .catch(() => (cuantosSensibles = 0));
    papelera()
      .then((p) => (enPapelera = p.length))
      .catch(() => (enPapelera = 0));
  });

  const carpetas = $derived((app.raiz?.rows ?? []).filter((r) => r.kind === "folder"));

  const colecciones = $derived(app.colecciones);

  // ── arrastrar a una colección (ADR-0033): cada una es un destino y, mientras
  // arrastras, al final sale «Nueva colección». Al soltar ahí, su nombre.
  const arrastre = $derived(app.arrastre);
  const pendiente = $derived(app.pendiente);
  let nombre = $state("");
  let creando = $state(false);
  let campo: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (pendiente) {
      nombre = "";
      campo?.focus();
    }
  });

  async function crearNueva(): Promise<void> {
    if (!nombre.trim() || creando) return;
    creando = true;
    await app.crearPendiente(nombre);
    creando = false;
  }

  function teclaNueva(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      void crearNueva();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      app.pendiente = null;
    }
  }

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
    <button class="item hov-fondo" class:activo={app.vista === "colecciones"} onclick={() => app.irVista("colecciones")}>
      <Layers size={17} />
      <span>Colecciones</span>
      {#if colecciones.length > 0}<span class="cuenta">{colecciones.length}</span>{/if}
    </button>
    <button class="item hov-fondo" class:activo={app.vista === "sensibles"} onclick={() => app.irVista("sensibles")}>
      <Lock size={17} />
      <span>Sensibles</span>
      <span class="cuenta">{cuantosSensibles}</span>
    </button>
    <button class="item hov-fondo" class:activo={app.vista === "papelera"} onclick={() => app.irVista("papelera")}>
      <Trash2 size={17} />
      <span>Papelera</span>
      {#if enPapelera > 0}<span class="cuenta">{enPapelera}</span>{/if}
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

  {#if colecciones.length > 0 || arrastre || app.pendiente}
    <div class="grupo carpetas">
      <div class="rotulo cabeza">Tus colecciones</div>
      {#each colecciones as c (c.id)}
        <button
          class="item hov-fondo"
          class:activo={app.vista === "coleccion" && app.abierta?.id === c.id}
          class:blanco={arrastre?.sobre === c.id && arrastre.desde !== c.id}
          class:origen={arrastre?.desde === c.id}
          data-soltar={c.id}
          onclick={() => app.abrirColeccion(c.id)}
          title={c.name}
        >
          <Layers size={17} />
          <span class="nombre">{c.name}</span>
          <span class="cuenta">{c.documents}</span>
        </button>
      {/each}
      {#if arrastre}
        <div class="item soltar-nueva" class:blanco={arrastre.sobre === "nueva"} data-soltar="nueva">
          <LayersPlus size={17} />
          <span>Nueva colección</span>
        </div>
      {:else if pendiente}
        <div class="nueva">
          <input
            bind:this={campo}
            bind:value={nombre}
            onkeydown={teclaNueva}
            class="campo"
            placeholder="Nombre de la colección"
            aria-label="Nombre de la colección nueva"
            maxlength="80"
          />
          <div class="con">
            <span>con {pendiente.docs.length === 1 ? `«${pendiente.etiqueta}»` : pendiente.etiqueta}</span>
            <button class="dejar hov-fondo" onclick={() => (app.pendiente = null)} aria-label="Dejarlo" title="Dejarlo · Esc">
              <X size={14} />
            </button>
          </div>
        </div>
      {/if}
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
        class:alerta={!!app.seguridad && !app.seguridad.cifrada}
        onclick={() => (app.panelSeguridad = true)}
        aria-label="Seguridad: cifrado, clave de recuperación y PIN"
        title="Seguridad"
      >
        <ShieldCheck size={15} />
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
  /* Con muchas carpetas y colecciones, la barra se desplaza: nada se encoge
     (sin esto, «Buscar» se aplastaba hasta la mitad). */
  .lateral > :global(*) {
    flex-shrink: 0;
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
  /* Arrastrando (ADR-0033): donde caería se ilumina; la colección de la que
     sale, no, porque ahí no pasaría nada. */
  .item.blanco {
    background: var(--sel);
    color: var(--acento);
    box-shadow: inset 0 0 0 1px var(--acento);
  }
  .item.origen {
    opacity: 0.5;
  }
  .soltar-nueva {
    border: 1px dashed var(--linea);
    color: var(--acento);
  }
  .nueva {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 2px 2px;
  }
  .nueva .campo {
    height: 36px;
    padding: 0 10px;
    font-size: 13.5px;
  }
  .con {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-left: 4px;
    font-size: 12px;
    color: var(--tenue);
  }
  .con span {
    flex-grow: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dejar {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    flex-shrink: 0;
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
  .icono.alerta {
    color: var(--peligro);
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
