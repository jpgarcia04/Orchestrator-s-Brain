<!--
  La miniatura de un archivo en la cuadrícula.

  Las fotos se enseñan de verdad, pero **sólo cuando entran en pantalla** —cada
  una cuesta ~25 ms y ~85 KB, y una carpeta de capturas tiene cientos— y nunca
  si son sensibles o si están en la nube: leer un archivo de OneDrive que no
  está en el disco lo descargaría. El resto de documentos es un papel con el
  color de su tipo.
-->
<script lang="ts" module>
  /** Las miniaturas ya pedidas, para no volver a pedirlas al cambiar de carpeta. */
  const cache = new Map<string, string>();
</script>

<script lang="ts">
  import ImageIcon from "@lucide/svelte/icons/image";
  import Lock from "@lucide/svelte/icons/lock";
  import { previsualizar } from "../lib/nucleo";
  import type { Fila } from "../lib/nucleo";
  import { colorDeTipo, tipoDe } from "../lib/nombres";

  let { fila }: { fila: Fila } = $props();

  const tipo = $derived(tipoDe(fila.label));
  const sensible = $derived(fila.sensitivity !== "normal");
  const local = $derived(fila.availability === "local");

  let imagen = $state<string | null>(null);
  let caja: HTMLElement | undefined = $state();

  $effect(() => {
    const id = fila.id;
    // Lo que se vuelve sensible —una carpeta que acabas de marcar— deja de
    // enseñarse aunque ya estuviera pedido, y la copia se olvida.
    if (sensible) cache.delete(id);
    imagen = sensible ? null : (cache.get(id) ?? null);
    if (tipo !== "IMG" || sensible || !local || imagen || !caja) return;
    const vigia = new IntersectionObserver((vistos) => {
      if (!vistos.some((v) => v.isIntersecting)) return;
      vigia.disconnect();
      previsualizar(id)
        .then((v) => {
          if (v.kind === "image" && v.image) {
            cache.set(id, v.image);
            if (fila.id === id) imagen = v.image;
          }
        })
        .catch(() => {});
    });
    vigia.observe(caja);
    return () => vigia.disconnect();
  });
</script>

<span class="miniatura" bind:this={caja}>
  {#if imagen}
    <img src={imagen} alt="" />
  {:else if tipo === "IMG"}
    <span class="foto" class:velada={sensible}><ImageIcon size={26} strokeWidth={1.5} /></span>
  {:else}
    <span class="papel" class:velada={sensible}>
      <span class="cabeza" style:background={colorDeTipo(tipo)}></span>
      <span class="linea" style:width="100%"></span>
      <span class="linea" style:width="88%"></span>
      <span class="linea" style:width="94%"></span>
      <span class="linea" style:width="60%"></span>
      <span class="linea hueco" style:width="90%"></span>
      <span class="linea" style:width="76%"></span>
    </span>
  {/if}
  {#if sensible}
    <span class="candado" title="Sensible: su contenido se enseña sólo si lo pides">
      <Lock size={14} strokeWidth={1.8} />
    </span>
  {/if}
</span>

<style>
  .miniatura {
    position: relative;
    display: grid;
    place-items: center;
    height: 150px;
    overflow: hidden;
    background: var(--chip);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .papel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 96px;
    height: 124px;
    padding: 13px 11px;
    border-radius: 3px;
    background: var(--papel);
    box-shadow: 0 8px 18px -8px var(--sombra);
  }

  .cabeza {
    height: 5px;
    width: 72%;
    border-radius: 2px;
  }

  .linea {
    height: 3px;
    border-radius: 2px;
    background: var(--papel-linea);
  }

  .hueco {
    margin-top: 6px;
  }

  .foto {
    display: grid;
    place-items: center;
    width: 138px;
    height: 90px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--tipo-img) 45%, var(--tarjeta));
    color: rgba(255, 255, 255, 0.85);
    box-shadow: 0 8px 18px -8px var(--sombra);
  }

  .velada {
    filter: blur(4px);
    opacity: 0.8;
  }

  .candado {
    position: absolute;
    right: 10px;
    top: 10px;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--tarjeta);
    color: var(--tenue);
    box-shadow: 0 2px 8px -2px var(--sombra);
  }
</style>
