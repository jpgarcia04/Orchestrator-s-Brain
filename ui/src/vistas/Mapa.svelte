<!--
  El mapa: la galaxia como **vista propia**, enlazada con el explorador.

  Sin foco, el cielo: cada estrella es una carpeta de tu bóveda —su tamaño es
  cuántos documentos tiene, su color cuándo tocaste algo ahí (`EXPERIENCE.md`
  §1)— y pulsarla abre esa carpeta en el explorador. Con foco («ver en el mapa»
  desde un documento), su vecindario: lo que se relaciona con él, y pulsar un
  documento lo enseña en la vista previa sin salir del mapa.

  Desde una colección (ADR-0031), el mismo cielo con **sus estrellas
  encendidas** —las carpetas donde están sus documentos— y las demás
  atenuadas. No se mueve ninguna: es una lente encima, no otro mapa.

  El dibujo es el de siempre (`lib/mapa.ts`, PixiJS); esto sólo le da sitio,
  le dice qué pintar y le quita el lienzo al salir.
-->
<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import LocateFixed from "@lucide/svelte/icons/locate-fixed";
  import { onMount, tick } from "svelte";
  import { app } from "../lib/app.svelte";
  import { Mapa } from "../lib/mapa";
  import { cielo, coleccion, lente } from "../lib/nucleo";
  import type { Abierta, Cielo } from "../lib/nucleo";
  import { nombreBonito, tituloDe } from "../lib/nombres";

  let lienzo: HTMLCanvasElement;
  let titulo: HTMLDivElement | undefined = $state();
  let leyenda: HTMLDivElement | undefined = $state();
  let mapa = $state<Mapa | null>(null);
  let movida = $state(false);
  let foco = $state("");
  /** Se está viendo la lente de una colección, no la de un documento. */
  let coleccionando = $state(false);
  let resumen = $state("");
  let error = $state("");

  onMount(() => {
    const m = new Mapa(lienzo);
    let vivo = true;
    m.onElegir = (p) => {
      if (!p) return;
      if (p.tipo === "region") {
        void app.abrirCarpeta(p.region.id);
      } else if (p.nodo.facets.includes("region")) {
        // Una colección tuya en la lente de un documento (ADR-0031).
        void app.abrirColeccion(p.nodo.id);
      } else if (p.nodo.facets.includes("container")) {
        void app.abrirCarpeta(p.nodo.id);
      } else {
        m.seleccionar(p.nodo.id);
        app.ver(p.nodo.id);
      }
    };
    m.onCamara = (v) => (movida = v);
    m.init()
      .then(() => {
        if (vivo) mapa = m;
        else m.destruir();
      })
      .catch((e) => (error = `No se pudo dibujar el mapa — ${e}`));
    return () => {
      vivo = false;
      mapa?.destruir();
      mapa = null;
    };
  });

  // Qué se dibuja: el cielo o la lente de alguien. Se repide tras reindexar.
  let pedido = 0;
  $effect(() => {
    const m = mapa;
    const quien = app.lente;
    const col = app.lenteColeccion;
    void app.version;
    if (m) void pintar(m, quien, col, ++pedido);
  });

  /**
   * Las estrellas que enciende una colección: por cada documento, la primera
   * carpeta por encima de él que el cielo tenga abierta (la más cercana).
   */
  function estrellas(c: Cielo, a: Abierta): { ids: Set<string>; docs: number } {
    const abiertas = new Set(c.regions.map((r) => r.id));
    const ids = new Set<string>();
    let docs = 0;
    for (const m of a.rows) {
      const r = m.ancestors.find((x) => abiertas.has(x));
      if (r) {
        ids.add(r);
        docs++;
      }
    }
    return { ids, docs };
  }

  async function pintar(m: Mapa, quien: string | null, col: string | null, mio: number): Promise<void> {
    try {
      if (col && !quien) {
        const [c, a] = await Promise.all([cielo(30), coleccion(col)]);
        if (mio !== pedido) return;
        const e = estrellas(c, a);
        foco = a.name;
        coleccionando = true;
        const n = a.rows.length;
        resumen =
          n === 0
            ? "Todavía no tiene documentos en tu bóveda"
            : `${n === 1 ? "1 documento" : `${n} documentos`} en ${e.ids.size === 1 ? "1 estrella" : `${e.ids.size} estrellas`}`;
        await encajar(m);
        m.cielo(c);
        m.encender(e.ids);
      } else if (quien) {
        coleccionando = false;
        const l = await lente([quien], 2, 300);
        if (mio !== pedido) return; // llegó tarde
        const n = l.nodes.find((x) => x.id === quien);
        foco = n ? (n.facets.includes("container") ? nombreBonito(n.label) : tituloDe(n.label).titulo) : "";
        resumen = `${l.nodes.length} cosas relacionadas${l.truncated ? ` de ${l.reached}` : ""}`;
        await encajar(m);
        m.mostrar(l);
        m.seleccionar(quien);
      } else {
        const c = await cielo(30);
        if (mio !== pedido) return;
        foco = "";
        coleccionando = false;
        resumen = `${c.regions.length === 1 ? "1 región" : `${c.regions.length} regiones`} · ${c.files === 1 ? "1 documento" : `${c.files.toLocaleString("es")} documentos`}`;
        await encajar(m);
        m.cielo(c);
      }
    } catch (e) {
      error = String(e);
    }
  }

  /** Le dice al mapa qué tapan el título y la leyenda, ya con el texto nuevo. */
  async function encajar(m: Mapa): Promise<void> {
    await tick();
    const alto = lienzo.parentElement?.clientHeight ?? 0;
    m.huecos = {
      arriba: titulo ? titulo.offsetTop + titulo.offsetHeight + 24 : 0,
      abajo: leyenda ? alto - leyenda.offsetTop + 16 : 0,
    };
  }
</script>

<div class="mapa">
  <canvas bind:this={lienzo}></canvas>

  <div class="encima">
    <div class="titulo" bind:this={titulo}>
      {#if foco && coleccionando}
        {@const col = app.lenteColeccion}
        <button class="volver hov" onclick={() => col && app.abrirColeccion(col)}>
          <ArrowLeft size={15} /> Volver a la colección
        </button>
        <h1 class="titular">«{foco}» en tu bóveda</h1>
        <p>Las estrellas encendidas son las carpetas donde están sus documentos; no se ha movido ninguna. Pulsa una para abrirla.</p>
      {:else if foco}
        <button class="volver hov" onclick={() => (app.lente = null)}>
          <ArrowLeft size={15} /> El mapa completo
        </button>
        <h1 class="titular">Alrededor de «{foco}»</h1>
        <p>Lo que se relaciona con esto: su carpeta, los duplicados, lo que lleva dentro o dónde está, tus colecciones, lo que se tocó a la vez. Pulsa un documento para verlo.</p>
      {:else}
        <h1 class="titular">Tu bóveda, vista desde arriba</h1>
        <p>Cada estrella es una carpeta: su tamaño es cuántos documentos tiene y su color, cuándo tocaste algo ahí. Pulsa una para abrirla.</p>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
    </div>

    <div class="leyenda" bind:this={leyenda}>
      <span class="resumen">{resumen}</span>
      <span><i class="punto calido"></i>tocado hace poco</span>
      <span><i class="punto frio"></i>hace meses</span>
      {#if foco && !coleccionando}<span><i class="trazo"></i>seguro · <i class="trazo punteado"></i>inferido</span>{/if}
    </div>

    {#if movida}
      <button class="ver-todo hov" onclick={() => mapa?.encuadre()}>
        <LocateFixed size={15} /> Ver todo
      </button>
    {/if}
  </div>
</div>

<style>
  .mapa {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: var(--mapa);
  }
  canvas {
    display: block;
  }

  /* Lo que va encima del lienzo no le roba el ratón, salvo sus botones. */
  .encima {
    position: absolute;
    inset: 0;
    pointer-events: none;
    color: #e7ecf3;
  }
  .encima button {
    pointer-events: auto;
  }

  .titulo {
    position: absolute;
    left: 36px;
    top: 30px;
    max-width: 460px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .titulo h1 {
    font-size: 36px;
    text-shadow: 0 2px 12px rgba(0, 0, 0, 0.8);
  }
  .titulo p {
    font-size: 14px;
    color: #a9b5c6;
    text-shadow: 0 1px 6px rgba(0, 0, 0, 0.9);
  }
  .volver,
  .ver-todo {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 12px;
    border-radius: 999px;
    background: rgba(17, 25, 40, 0.9);
    border: 1px solid rgba(150, 180, 225, 0.26);
    color: #e7ecf3;
    font-size: 13px;
  }
  .error {
    color: #f0a094 !important;
  }

  .leyenda {
    position: absolute;
    left: 36px;
    bottom: 24px;
    display: flex;
    align-items: center;
    gap: 18px;
    font-size: 12.5px;
    color: #a9b5c6;
  }
  .leyenda span {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .resumen {
    color: #c9d4e3;
  }
  .punto {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .punto.calido {
    background: #f6cb8c;
    box-shadow: 0 0 8px 2px rgba(246, 203, 140, 0.5);
  }
  .punto.frio {
    background: #8faed8;
  }
  .trazo {
    width: 20px;
    height: 0;
    border-top: 1.5px solid rgba(143, 178, 221, 0.8);
  }
  .trazo.punteado {
    border-top-style: dashed;
  }

  .ver-todo {
    position: absolute;
    right: 24px;
    bottom: 20px;
  }
</style>
