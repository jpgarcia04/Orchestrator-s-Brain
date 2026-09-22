<!--
  El buscador: `Ctrl K`, o empezar a escribir en cualquier sitio.

  Es la barra de ADR-0004 y ADR-0017 con otra forma: el camino de las cuarenta
  veces al día sigue siendo **escribir**, sin un gesto delante. Cada resultado
  dice por qué está —la frase que coincide, y si coincidió por las palabras o
  por el significado (I9)—, y al abrirlo la vista previa empieza justo ahí.
-->
<script lang="ts">
  import Clock from "@lucide/svelte/icons/clock";
  import Orbit from "@lucide/svelte/icons/orbit";
  import Search from "@lucide/svelte/icons/search";
  import Tipo from "./Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import { recientes, vistazo } from "../lib/nucleo";
  import type { Fila, Hit } from "../lib/nucleo";
  import { carpetaDe, tipoDe, tituloDe } from "../lib/nombres";

  let q = $state(app.semilla);
  let resultados = $state<Hit[]>([]);
  let semantica = $state(true);
  let buscado = $state("");
  let marcado = $state(0);
  let ultimos = $state<Fila[]>([]);
  let campo: HTMLInputElement | undefined = $state();

  $effect(() => {
    campo?.focus();
  });

  recientes(5).then((r) => (ultimos = r)).catch(() => {});

  // 90 ms: la consulta híbrida completa se midió en 43 ms (M4), así que la
  // espera puede quedarse por debajo de lo que se percibe como retardo.
  let generacion = 0;
  $effect(() => {
    const texto = q.trim();
    const mia = ++generacion;
    if (!texto) {
      resultados = [];
      buscado = "";
      return;
    }
    const t = window.setTimeout(() => {
      vistazo(texto, 12)
        .then((v) => {
          if (mia !== generacion) return; // llegó tarde
          resultados = v.answer.hits;
          semantica = v.answer.semantic;
          buscado = texto;
          marcado = 0;
        })
        .catch(() => {
          if (mia === generacion) resultados = [];
        });
    }, 90);
    return () => window.clearTimeout(t);
  });

  /** Lo que se puede elegir ahora mismo, en orden. */
  const opciones = $derived(
    q.trim()
      ? resultados.map((h) => ({ id: h.entity, consulta: buscado }))
      : ultimos.map((f) => ({ id: f.id, consulta: "" })),
  );

  function cerrar(): void {
    app.buscando = false;
  }

  function elegir(i: number): void {
    const o = opciones[i];
    if (!o) return;
    cerrar();
    void app.irA(o.id, o.consulta);
  }

  function tecla(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      cerrar();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (opciones.length) marcado = (marcado + 1) % opciones.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (opciones.length) marcado = (marcado - 1 + opciones.length) % opciones.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      elegir(marcado);
    }
  }

  function como(h: Hit): string {
    return h.ranks.some(([rama]) => rama === "bm25") ? "por palabras" : "por significado";
  }

  function irAlMapa(): void {
    cerrar();
    app.irVista("mapa");
  }
</script>

<div class="fondo">
  <button class="cortina" onclick={cerrar} aria-label="Cerrar la búsqueda" tabindex="-1"></button>
  <section class="buscador entra" aria-label="Buscar">
    <div class="entrada">
      <Search size={20} />
      <input
        bind:this={campo}
        bind:value={q}
        onkeydown={tecla}
        placeholder="Busca por nombre o por lo que significa…"
        aria-label="Buscar en tu bóveda"
        spellcheck="false"
        autocomplete="off"
      />
      <kbd>Esc</kbd>
    </div>

    <div class="cuerpo">
      {#if !q.trim()}
        {#if ultimos.length > 0}
          <div class="rotulo seccion">Recientes</div>
          {#each ultimos as f, i (f.id)}
            <button
              class="opcion"
              class:marcada={i === marcado}
              onclick={() => elegir(i)}
              onmouseenter={() => (marcado = i)}
            >
              <Tipo tipo={tipoDe(f.label)} />
              <span class="texto">
                <span class="nombre">{tituloDe(f.label).titulo}</span>
                <span class="sub">{carpetaDe(f.path)}</span>
              </span>
              <Clock size={14} />
            </button>
          {/each}
        {/if}
        <div class="rotulo seccion">Ir a</div>
        <button class="opcion" onclick={irAlMapa}>
          <Orbit size={18} />
          <span class="texto"><span class="nombre">El mapa de tu bóveda</span></span>
        </button>
      {:else if resultados.length > 0}
        {#each resultados as h, i (h.entity)}
          <button
            class="opcion"
            class:marcada={i === marcado}
            onclick={() => elegir(i)}
            onmouseenter={() => (marcado = i)}
          >
            <Tipo tipo={tipoDe(h.label)} />
            <span class="texto">
              <span class="nombre">{tituloDe(h.label).titulo}</span>
              <span class="sub">{carpetaDe(h.path)}{h.snippet ? ` · ${h.snippet}` : ""}</span>
            </span>
            <span class="como" class:significado={como(h) === "por significado"}>{como(h)}</span>
          </button>
        {/each}
      {:else if buscado}
        <p class="nada">
          No encuentro nada con «{buscado}». Busqué en los nombres y en el texto de tus documentos{semantica
            ? ", también por significado"
            : ""}; las fotos y los PDF escaneados todavía no tienen texto que leer.
        </p>
      {/if}
    </div>

    <div class="pie">
      <span>↑↓ moverse</span>
      <span>Intro abrir</span>
      <span>Esc cerrar</span>
      <span class="derecha">{semantica ? "Palabras y significado · en tu computadora" : "Sólo palabras · falta el modelo local"}</span>
    </div>
  </section>
</div>

<style>
  .fondo {
    position: fixed;
    inset: 0;
    z-index: 20;
  }
  .cortina {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: default;
    background: rgba(4, 7, 12, 0.55);
  }

  .buscador {
    position: absolute;
    top: 12vh;
    left: 0;
    right: 0;
    margin: 0 auto;
    width: min(680px, calc(100vw - 32px));
    border-radius: 18px;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--linea);
    box-shadow: 0 40px 120px -20px rgba(0, 0, 0, 0.6);
  }

  .entrada {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 64px;
    padding: 0 18px;
    border-bottom: 1px solid var(--linea);
    color: var(--tenue);
  }
  input {
    flex-grow: 1;
    height: 40px;
    border: 0;
    outline: none;
    background: transparent;
    font-size: 17px;
    color: var(--tinta);
  }
  input::placeholder {
    color: var(--tenue);
  }

  .cuerpo {
    max-height: min(440px, 56vh);
    overflow-y: auto;
    padding: 8px;
  }
  .seccion {
    padding: 10px 10px 6px;
  }

  .opcion {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px;
    border-radius: 10px;
    color: var(--tenue);
  }
  .opcion.marcada {
    background: var(--sel);
  }
  .texto {
    flex-grow: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .nombre {
    color: var(--tinta);
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .como {
    flex-shrink: 0;
    font-size: 12px;
  }
  .como.significado {
    color: var(--acento);
  }
  .nada {
    padding: 14px 12px 16px;
    line-height: 1.55;
    color: var(--tenue);
  }

  .pie {
    display: flex;
    gap: 16px;
    padding: 10px 18px;
    border-top: 1px solid var(--linea);
    font-size: 12px;
    color: var(--tenue);
  }
  .derecha {
    margin-left: auto;
  }
</style>
