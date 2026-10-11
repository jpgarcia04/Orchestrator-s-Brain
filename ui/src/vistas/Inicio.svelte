<!--
  El inicio: lo primero que ves al abrir.

  Nada de métricas ni avisos que reclamen atención (`EXPERIENCE.md` §7): un
  saludo, dónde buscar, lo último que tocaste, tu mapa en pequeño y tus
  carpetas. Si la bóveda está vacía, lo único que se enseña es cómo empezarla.

  «Caduca pronto» (ADR-0023) sólo aparece si hay algo que decir: lo que
  caduca en 90 días o ya caducó. No notifica nada; está donde miras.
-->
<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Lock from "@lucide/svelte/icons/lock";
  import Search from "@lucide/svelte/icons/search";
  import Tipo from "../componentes/Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import { cielo, recientes, vencimientos } from "../lib/nucleo";
  import type { Cielo, Fila, Vencimiento } from "../lib/nucleo";
  import {
    carpetaDe,
    diasHasta,
    fechaIso,
    haceCuanto,
    nombreBonito,
    plazo,
    saludo,
    tipoDe,
    tituloDe,
  } from "../lib/nombres";

  let ultimos = $state<Fila[]>([]);
  let mapa = $state<Cielo | null>(null);
  let caducan = $state<Vencimiento[]>([]);
  let todos = $state(false);

  $effect(() => {
    void app.version;
    recientes(6).then((r) => (ultimos = r)).catch(() => (ultimos = []));
    cielo(30).then((c) => (mapa = c)).catch(() => (mapa = null));
    vencimientos().then((v) => (caducan = v)).catch(() => (caducan = []));
  });

  /** Filas que se ven sin desplegar. Más, y la tarjeta se come el inicio. */
  const PRIMERAS = 4;
  const visibles = $derived(todos ? caducan : caducan.slice(0, PRIMERAS));

  /** Días en tu zona horaria; si no hay fecha (PIN), no se sabe. */
  const diasDe = (v: Vencimiento) => (v.date ? diasHasta(v.date) : null);

  const carpetas = $derived((app.raiz?.rows ?? []).filter((r) => r.kind === "folder"));
  const total = $derived(
    (app.raiz?.rows ?? []).reduce((n, r) => n + (r.kind === "folder" ? r.weight : 1), 0),
  );

  /** Las estrellas del mapa pequeño: la posición de partida de cada región. */
  const estrellas = $derived.by(() => {
    if (!mapa) return [];
    const { oldest, newest } = mapa;
    return mapa.regions.map((r) => {
      const calor = newest > oldest && r.recency > 0 ? Math.pow((r.recency - oldest) / (newest - oldest), 2) : 0;
      const d = Math.min(20, 5 + Math.sqrt(r.weight) * 2.2);
      return {
        id: r.id,
        x: 50 + r.seed[0] * 40,
        y: 50 + r.seed[1] * 38,
        d,
        color: calor > 0.5 ? "#F6CB8C" : calor > 0.15 ? "#E6DCCB" : "#8FAED8",
      };
    });
  });
</script>

<div class="inicio entra">
  <div class="saludo">
    <h1 class="titular">{saludo()}</h1>
    {#if !app.vacia}
      <p>{total === 1 ? "1 documento" : `${total} documentos`} en tu bóveda</p>
    {/if}
  </div>

  {#if app.vacia}
    <section class="vacia" aria-label="Empezar">
      <h2 class="titular">Tu bóveda está vacía</h2>
      <p>
        Elige la carpeta donde vas a guardar tus documentos —pasaporte, seguros, matrícula,
        contratos— y la leeré entera. Todo se queda en tu computadora.
      </p>
      <button class="primario hov" onclick={() => app.anadirCarpeta()}>
        <FolderPlus size={18} />
        Elegir carpeta
      </button>
      <p class="pista">
        También desde la terminal: <code>obrain indexar "C:\Users\tú\Segundo Cerebro"</code>
      </p>
    </section>
  {:else}
    <button class="buscar hov" onclick={() => app.abrirBuscador()}>
      <Search size={20} />
      <span>Busca por nombre o por lo que significa…</span>
      <kbd>Ctrl K</kbd>
    </button>

    {#if caducan.length > 0}
      <section aria-label="Caduca pronto" class="bloque">
        <h2 class="rotulo">Caduca pronto</h2>
        <div class="caducan">
          {#each visibles as v (v.id)}
            {@const d = diasDe(v)}
            <button
              class="reciente hov-fondo"
              class:elegida={app.seleccion === v.id}
              onclick={() => app.ver(v.id)}
              title={v.path}
            >
              <Tipo tipo={tipoDe(v.label)} />
              <span class="texto">
                <span class="nombre">{tituloDe(v.label).titulo}</span>
                <span class="donde">
                  {nombreBonito(v.folder ?? carpetaDe(v.path))} ·
                  {#if v.locked}
                    fecha protegida con tu PIN
                  {:else if v.date}
                    {d !== null && d < 0 ? "caducó el" : "caduca el"} {fechaIso(v.date)}{v.by_user ? " · lo dijiste tú" : ""}
                  {/if}
                </span>
              </span>
              {#if v.locked}
                <span class="plazo"><Lock size={12} /> PIN</span>
              {:else if d !== null}
                <span class="plazo" class:caducado={d < 0} class:cerca={d >= 0 && d <= 30}>{plazo(d)}</span>
              {/if}
            </button>
          {/each}
          {#if caducan.length > PRIMERAS}
            <button class="mas hov-fondo" onclick={() => (todos = !todos)}>
              {todos ? "Ver menos" : `Ver los ${caducan.length}`}
            </button>
          {/if}
        </div>
      </section>
    {/if}

    <div class="dos">
      <section aria-label="Recientes" class="bloque">
        <h2 class="rotulo">Recientes</h2>
        {#each ultimos as f (f.id)}
          {@const t = tituloDe(f.label)}
          <button
            class="reciente hov-fondo"
            class:elegida={app.seleccion === f.id}
            onclick={() => app.ver(f.id)}
            title={f.path}
          >
            <Tipo tipo={tipoDe(f.label)} />
            <span class="texto">
              <span class="nombre">{t.titulo}</span>
              <span class="donde">{carpetaDe(f.path)} · {haceCuanto(f.recency)}</span>
            </span>
          </button>
        {:else}
          <p class="nada">Todavía nada.</p>
        {/each}
      </section>

      <section aria-label="Tu mapa" class="bloque">
        <h2 class="rotulo">Tu mapa</h2>
        <button class="mini hov" onclick={() => app.irVista("mapa")} aria-label="Abrir el mapa">
          {#each estrellas as e (e.id)}
            <span
              class="estrella"
              style:left="{e.x}%"
              style:top="{e.y}%"
              style:width="{e.d}px"
              style:height="{e.d}px"
              style:background={e.color}
              style:box-shadow="0 0 {e.d * 1.4}px {e.d / 3}px {e.color}66"
            ></span>
          {/each}
          <span class="pie-mini">
            <span>{mapa ? (mapa.regions.length === 1 ? "1 región" : `${mapa.regions.length} regiones`) : ""}</span>
            <span class="abrir">Abrir el mapa <ChevronRight size={14} /></span>
          </span>
        </button>
      </section>
    </div>

    {#if carpetas.length > 0}
      <section aria-label="Tu bóveda" class="bloque">
        <h2 class="rotulo">Tu bóveda</h2>
        <div class="carpetas">
          {#each carpetas as c (c.id)}
            <button class="carpeta hov" onclick={() => app.abrirCarpeta(c.id)} title={c.path}>
              <Folder size={20} />
              <span class="nombre">{nombreBonito(c.label)}</span>
              <span class="cuenta">{c.weight}</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .inicio {
    display: flex;
    flex-direction: column;
    gap: 34px;
    padding: 40px 44px 48px;
    max-width: 1100px;
  }

  .saludo {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .saludo h1 {
    font-size: 48px;
  }
  .saludo p {
    color: var(--tenue);
    font-size: 15px;
  }

  .buscar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 56px;
    max-width: 720px;
    padding: 0 18px;
    border-radius: 14px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    color: var(--tenue);
    font-size: 16px;
    box-shadow: 0 10px 34px -14px var(--sombra);
  }
  .buscar span {
    flex-grow: 1;
  }

  .dos {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 28px;
  }

  .bloque {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bloque h2 {
    margin-bottom: 6px;
  }

  .reciente {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 10px;
    border-radius: 10px;
  }
  .reciente.elegida {
    background: var(--sel);
  }
  .texto {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .nombre {
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .donde,
  .nada {
    font-size: 12.5px;
    color: var(--tenue);
  }

  .caducan {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(420px, 1fr));
    gap: 2px 14px;
    padding: 6px;
    border-radius: 16px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  .caducan .texto {
    flex-grow: 1;
  }
  .caducan .donde {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .plazo {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--chip);
    color: var(--tenue);
    font-size: 12px;
    white-space: nowrap;
  }
  .plazo.cerca {
    background: var(--calido-fondo);
    color: var(--calido);
  }
  .plazo.caducado {
    background: var(--peligro-fondo);
    color: var(--peligro);
  }
  .mas {
    grid-column: 1 / -1;
    padding: 8px 10px;
    border-radius: 10px;
    color: var(--acento);
    font-size: 13px;
    text-align: left;
  }

  .mini {
    position: relative;
    display: block;
    height: 232px;
    border-radius: 16px;
    overflow: hidden;
    background: var(--mapa);
    border: 1px solid var(--linea);
  }
  .estrella {
    position: absolute;
    border-radius: 50%;
    transform: translate(-50%, -50%);
  }
  .pie-mini {
    position: absolute;
    left: 16px;
    right: 16px;
    bottom: 14px;
    display: flex;
    align-items: center;
    font-size: 13px;
    color: #c9d4e3;
  }
  .pie-mini span:first-child {
    flex-grow: 1;
  }
  .abrir {
    display: flex;
    align-items: center;
    gap: 4px;
    color: #f2c27b;
  }

  .carpetas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 10px;
  }
  .carpeta {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 16px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    color: var(--tenue);
  }
  .carpeta .nombre {
    flex-grow: 1;
    color: var(--tinta);
  }
  .cuenta {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .vacia {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    max-width: 620px;
    padding: 28px 30px;
    border-radius: 18px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  .vacia h2 {
    font-size: 30px;
  }
  .vacia p {
    color: var(--tenue);
    line-height: 1.55;
  }
  .primario {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 42px;
    padding: 0 18px;
    border-radius: 11px;
    background: var(--prim);
    color: var(--prim-tinta);
    font-weight: 500;
  }
  .pista {
    font-size: 12.5px;
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--tinta);
  }
</style>
