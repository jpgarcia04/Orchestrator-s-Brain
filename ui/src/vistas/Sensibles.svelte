<!--
  Lo sensible, junto. Son los documentos que la puerta de egreso retiene
  (`PROJECT-MASTER.md` §5.2): nunca salen de la máquina ni entran en un modelo
  remoto, y su contenido no se enseña hasta que lo pides.
-->
<script lang="ts">
  import Lock from "@lucide/svelte/icons/lock";
  import { app } from "../lib/app.svelte";
  import { sensibles } from "../lib/nucleo";
  import type { Fila } from "../lib/nucleo";
  import { carpetaDe, fecha, tituloDe } from "../lib/nombres";

  let filas = $state<Fila[] | null>(null);

  $effect(() => {
    void app.version;
    sensibles().then((s) => (filas = s)).catch(() => (filas = []));
  });
</script>

<div class="sensibles entra">
  <div class="titulo">
    <h1 class="titular">Sensibles</h1>
    <p>
      Nunca salen de tu computadora ni entran en ningún modelo remoto. Su contenido se enseña sólo
      cuando lo pides.
    </p>
  </div>

  {#if filas && filas.length === 0}
    <p class="nada">Ninguno de tus documentos está marcado como sensible.</p>
  {:else if filas}
    <div class="lista">
      {#each filas as f (f.id)}
        {@const t = tituloDe(f.label)}
        <button
          class="fila hov-fondo"
          class:elegida={app.seleccion === f.id}
          onclick={() => app.ver(f.id)}
          ondblclick={() => app.irA(f.id)}
          title={f.path}
        >
          <span class="primera">
            <Lock size={15} />
            <span class="nombre">{t.titulo}</span>
          </span>
          <span class="tenue">{carpetaDe(f.path)}</span>
          <span class="tenue">{t.fecha ?? fecha(f.recency)}</span>
          <span class="nivel">{f.sensitivity === "vault" ? "Bóveda" : "Sensible"}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sensibles {
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
  .nada,
  .tenue {
    color: var(--tenue);
  }
  .tenue {
    font-size: 13px;
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
    grid-template-columns: minmax(0, 1fr) 150px 120px 80px;
    gap: 16px;
    align-items: center;
    padding: 13px 20px;
    border-bottom: 1px solid var(--linea);
  }
  .fila:last-child {
    border-bottom: 0;
  }
  .fila.elegida {
    background: var(--sel);
  }
  .primera {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    color: var(--tenue);
  }
  .nombre {
    color: var(--tinta);
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nivel {
    justify-self: end;
    font-size: 11.5px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--chip);
    color: var(--tenue);
  }
</style>
