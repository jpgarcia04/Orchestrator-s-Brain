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
  import Download from "@lucide/svelte/icons/download";
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
    if (app.seguridad?.reindexado_pausado) return { texto: "Sincronización en pausa", color: "var(--calido)" };
    if (app.vacia) return { texto: "Sin carpetas todavía", color: "var(--tenue)" };
    return { texto: "Al día", color: "var(--bien)" };
  });

  const ruta = $derived(app.info?.libraries.join("\n") ?? "");
</script>

<nav class="lateral" aria-label="Navegación">
  <div class="marca">
    <!-- El logo: un hemisferio de olas y otro de las mismas olas hechas estrellas. -->
    <svg viewBox="80 92 360 330" aria-hidden="true"><defs><clipPath id="marca-lh"><path d="M246,110 C206,96 150,112 124,150 C96,176 92,214 100,246 C88,288 104,336 136,362 C160,398 210,418 246,404 Z"/></clipPath><clipPath id="marca-rh"><path d="M266,110 C306,96 362,112 388,150 C416,176 420,214 412,246 C424,288 408,336 376,362 C352,398 302,418 266,404 Z"/></clipPath></defs><g clip-path="url(#marca-lh)" fill="none" stroke-width="30" stroke-linecap="round"><path d="M80,136 L87,140 L94,143 L102,146 L109,149 L117,150 L125,150 L133,149 L141,146 L148,143 L155,140 L162,136 L170,132 L177,129 L184,126 L192,124 L200,122 L208,122 L216,123 L223,126 L231,129 L238,132 L245,136 L252,139 L259,143" stroke="#36d6f5"/><path d="M80,204 L87,207 L95,210 L103,211 L111,211 L119,210 L126,208 L134,205 L141,202 L148,198 L155,194 L162,191 L170,188 L177,185 L185,184 L193,184 L201,185 L209,187 L216,190 L224,193 L231,197 L238,200 L245,204 L252,207 L260,210" stroke="#6099f6"/><path d="M80,271 L88,272 L96,273 L104,272 L112,270 L119,268 L126,264 L133,261 L140,257 L148,253 L155,250 L162,247 L170,246 L178,245 L186,246 L194,248 L201,250 L209,254 L216,257 L223,261 L230,265 L237,268 L245,271 L253,272 L261,273" stroke="#8b5cf6"/><path d="M80,334 L88,334 L96,333 L103,330 L111,327 L118,323 L125,320 L132,316 L139,312 L147,310 L155,308 L162,307 L170,307 L178,308 L186,311 L193,314 L200,318 L207,321 L215,325 L222,328 L229,331 L237,333 L245,334 L253,334 L261,333" stroke="#b65be7"/><path d="M80,395 L88,393 L95,390 L102,386 L109,382 L116,379 L124,375 L131,372 L139,370 L147,368 L155,368 L162,369 L170,371 L178,374 L185,378 L192,382 L199,385 L206,389 L213,392 L221,394 L229,396 L237,396 L245,395 L253,393 L260,390" stroke="#e05ad8"/></g><g clip-path="url(#marca-rh)"><circle cx="278" cy="149" r="14" fill="#3ed8f5" opacity="0.97"/><circle cx="307" cy="146" r="12" fill="#52dcf6" opacity="0.90"/><circle cx="334" cy="133" r="10" fill="#64dff7" opacity="0.84"/><circle cx="362" cy="123" r="8" fill="#77e3f8" opacity="0.78"/><circle cx="392" cy="127" r="6" fill="#8ae7f9" opacity="0.71"/><circle cx="418" cy="140" r="4" fill="#9cebfa" opacity="0.64"/><circle cx="279" cy="211" r="14" fill="#679ef6" opacity="0.97"/><circle cx="307" cy="201" r="12" fill="#76a7f7" opacity="0.90"/><circle cx="334" cy="188" r="10" fill="#84b0f8" opacity="0.84"/><circle cx="364" cy="184" r="8" fill="#94baf9" opacity="0.77"/><circle cx="392" cy="194" r="6" fill="#a3c4fa" opacity="0.71"/><circle cx="419" cy="207" r="4" fill="#b1cdfb" opacity="0.64"/><circle cx="280" cy="269" r="14" fill="#9063f6" opacity="0.97"/><circle cx="307" cy="257" r="12" fill="#9b72f7" opacity="0.91"/><circle cx="334" cy="246" r="10" fill="#a581f8" opacity="0.84"/><circle cx="364" cy="249" r="8" fill="#b191f9" opacity="0.77"/><circle cx="391" cy="262" r="6" fill="#bba0fa" opacity="0.71"/><circle cx="419" cy="272" r="4" fill="#c6affb" opacity="0.64"/><circle cx="279" cy="326" r="14" fill="#b962e8" opacity="0.97"/><circle cx="305" cy="312" r="12" fill="#c070ea" opacity="0.91"/><circle cx="334" cy="307" r="10" fill="#c780ec" opacity="0.84"/><circle cx="363" cy="316" r="8" fill="#ce90ef" opacity="0.77"/><circle cx="390" cy="329" r="6" fill="#d49ff1" opacity="0.71"/><circle cx="419" cy="334" r="4" fill="#dbaff3" opacity="0.64"/><circle cx="277" cy="381" r="14" fill="#e160d9" opacity="0.97"/><circle cx="305" cy="369" r="12" fill="#e46fdd" opacity="0.91"/><circle cx="334" cy="371" r="10" fill="#e780e1" opacity="0.84"/><circle cx="361" cy="384" r="8" fill="#ea8fe4" opacity="0.78"/><circle cx="389" cy="395" r="6" fill="#ed9ee8" opacity="0.71"/><circle cx="419" cy="392" r="4" fill="#f0aeec" opacity="0.64"/></g></svg>
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
    {#if app.versionNueva}
      <button
        class="version hov-fondo"
        disabled={app.instalandoVersion}
        onclick={() => app.instalarVersionNueva()}
        title={`Tienes la ${app.versionNueva.actual}. Al actualizar, Brain se cierra y vuelve a abrirse.`}
      >
        <Download size={15} />
        <span>
          {app.instalandoVersion
            ? `Descargando la ${app.versionNueva.nueva}…`
            : `Versión ${app.versionNueva.nueva} · Actualizar`}
        </span>
      </button>
    {/if}
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
    flex: none;
    width: 24px;
    height: 22px;
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
  .version {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
    padding: 4px 6px;
    border-radius: 8px;
    font-size: 12.5px;
    text-align: left;
    color: var(--acento);
    border: 1px solid var(--linea);
  }
  .version:disabled {
    color: var(--tenue);
    cursor: progress;
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
